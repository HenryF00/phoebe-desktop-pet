//! Native one-shot device location for the Agent.
//!
//! The Agent's `get_device_location` tool used to read a location that the user
//! had to attach manually through the chat window, because the WebView
//! `navigator.geolocation` API is not wired up by wry on macOS. This module
//! instead asks the operating system directly: CoreLocation on macOS and
//! Windows.Devices.Geolocation on Windows. The first request triggers the
//! system permission prompt; afterwards the OS returns the cached position.
//!
//! The result is rounded to 0.1 degrees (a city-level position) and never
//! tracked continuously, so it stays "one-shot coarse location".

/// A city-level position: latitude/longitude rounded to 0.1 degrees.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CoarseLocation {
    pub latitude: f64,
    pub longitude: f64,
    #[serde(rename = "accuracyMeters")]
    pub accuracy_meters: f64,
    #[serde(rename = "capturedAt")]
    pub captured_at: String,
}

/// Requests a single coarse location, blocking for up to ~15 seconds.
pub fn current_location(app: &tauri::AppHandle) -> Result<CoarseLocation, String> {
    current_location_impl(app)
}

#[cfg(target_os = "macos")]
fn current_location_impl(app: &tauri::AppHandle) -> Result<CoarseLocation, String> {
    use std::sync::mpsc;
    use std::time::Duration;
    let (sender, receiver) = mpsc::channel();
    app.run_on_main_thread(move || macos::start_request(sender))
        .map_err(|_| "无法调度定位请求".to_owned())?;
    let (latitude, longitude, accuracy_meters) = receiver
        .recv_timeout(Duration::from_secs(15))
        .map_err(|_| "定位超时；请确认系统定位服务已开启后重试".to_owned())??;
    Ok(CoarseLocation {
        latitude: (latitude * 10.0).round() / 10.0,
        longitude: (longitude * 10.0).round() / 10.0,
        accuracy_meters,
        captured_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::{mpsc, Mutex, OnceLock};

    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{define_class, msg_send, AnyThread};
    use objc2_core_location::{
        CLAuthorizationStatus, CLLocation, CLLocationManager, CLLocationManagerDelegate,
    };
    use objc2_foundation::{NSArray, NSError};

    pub type Pending = mpsc::Sender<Result<(f64, f64, f64), String>>;

    /// The one in-flight request. Only accessed from the main thread: the
    /// request is started via `run_on_main_thread` and finished from the
    /// delegate callback, which CoreLocation also delivers on the main thread.
    fn pending() -> &'static Mutex<Option<Pending>> {
        static PENDING: OnceLock<Mutex<Option<Pending>>> = OnceLock::new();
        PENDING.get_or_init(|| Mutex::new(None))
    }

    fn finish(result: Result<(f64, f64, f64), String>) {
        if let Ok(mut guard) = pending().lock() {
            if let Some(sender) = guard.take() {
                let _ = sender.send(result);
            }
        }
    }

    fn request_location(manager: &CLLocationManager) {
        let status = unsafe { manager.authorizationStatus() };
        match status {
            CLAuthorizationStatus::NotDetermined => unsafe { manager.requestWhenInUseAuthorization() },
            CLAuthorizationStatus::AuthorizedWhenInUse | CLAuthorizationStatus::AuthorizedAlways => {
                unsafe { manager.requestLocation() };
            }
            CLAuthorizationStatus::Denied | CLAuthorizationStatus::Restricted => {
                finish(Err(
                    "定位权限被拒绝；请在系统设置 → 隐私与安全性 → 定位服务中允许菲比助手".to_owned(),
                ));
            }
            _ => {}
        }
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "PhoebeLocationDelegate"]
        struct PhoebeLocationDelegate;

        unsafe impl NSObjectProtocol for PhoebeLocationDelegate {}

        unsafe impl CLLocationManagerDelegate for PhoebeLocationDelegate {
            #[unsafe(method(locationManager:didUpdateLocations:))]
            fn did_update_locations(&self, _manager: &CLLocationManager, locations: &NSArray<CLLocation>) {
                let result = locations
                    .firstObject()
                    .map(|location| {
                        let coordinate = unsafe { location.coordinate() };
                        let accuracy = unsafe { location.horizontalAccuracy() };
                        (coordinate.latitude, coordinate.longitude, accuracy)
                    })
                    .ok_or_else(|| "未收到定位结果".to_owned());
                finish(result);
            }

            #[unsafe(method(locationManager:didFailWithError:))]
            fn did_fail_with_error(&self, _manager: &CLLocationManager, error: &NSError) {
                let message = error.localizedDescription().to_string();
                finish(Err(format!("定位失败：{message}")));
            }

            #[unsafe(method(locationManagerDidChangeAuthorization:))]
            fn did_change_authorization(&self, manager: &CLLocationManager) {
                request_location(manager);
            }
        }
    );

    impl PhoebeLocationDelegate {
        fn new() -> Retained<Self> {
            let this = Self::alloc().set_ivars(());
            unsafe { msg_send![super(this), init] }
        }
    }

    pub fn start_request(sender: Pending) {
        let manager = unsafe { CLLocationManager::new() };
        let status = unsafe { manager.authorizationStatus() };
        match status {
            CLAuthorizationStatus::Denied | CLAuthorizationStatus::Restricted => {
                let _ = sender.send(Err(
                    "定位权限被拒绝；请在系统设置 → 隐私与安全性 → 定位服务中允许菲比助手".to_owned(),
                ));
                return;
            }
            _ => {}
        }
        *pending().lock().unwrap() = Some(sender);
        let delegate = PhoebeLocationDelegate::new();
        unsafe {
            manager.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            request_location(&manager);
        }
        // The delegate is a weak property on the manager, and the location
        // arrives asynchronously after this function returns. Keep both alive;
        // the frequency is one or two per conversation, so this tiny leak is
        // intentional and bounded in practice.
        std::mem::forget(manager);
        std::mem::forget(delegate);
    }
}

#[cfg(target_os = "windows")]
fn current_location_impl(_app: &tauri::AppHandle) -> Result<CoarseLocation, String> {
    use windows::Devices::Geolocation::{GeolocationAccessStatus, Geolocator};

    let geolocator = Geolocator::new().map_err(|_| "无法创建定位器".to_owned())?;
    let access = Geolocator::RequestAccessAsync()
        .map_err(|_| "无法请求定位权限".to_owned())?
        .get()
        .map_err(|_| "定位权限请求失败".to_owned())?;
    if access != GeolocationAccessStatus::Allowed {
        return Err("定位权限被拒绝；请在系统设置 → 隐私 → 位置中允许菲比助手".to_owned());
    }
    let position = geolocator
        .GetGeopositionAsync()
        .map_err(|_| "无法启动定位".to_owned())?
        .get()
        .map_err(|_| "定位超时".to_owned())?;
    let coordinate = position.Coordinate().map_err(|_| "定位结果不可用".to_owned())?;
    let point = coordinate.Point().map_err(|_| "定位结果不可用".to_owned())?;
    let basic = point.Position().map_err(|_| "定位结果不可用".to_owned())?;
    let accuracy_meters = coordinate.Accuracy().unwrap_or(0.0);
    Ok(CoarseLocation {
        latitude: (basic.Latitude * 10.0).round() / 10.0,
        longitude: (basic.Longitude * 10.0).round() / 10.0,
        accuracy_meters,
        captured_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn current_location_impl(_app: &tauri::AppHandle) -> Result<CoarseLocation, String> {
    Err("当前平台不支持设备定位".to_owned())
}
