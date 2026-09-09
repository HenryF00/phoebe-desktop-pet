#!/usr/bin/env python3
"""Build the native pet from repository sources; Xcode command line tools required."""
from pathlib import Path
import os
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]

def main():
    app = ROOT / 'dist/Roxy HD.app/Contents'
    for name in ['MacOS', 'Resources/frames']:
        (app / name).mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='roxy-build-') as temp:
        executable = Path(temp) / 'RoxyHD'
        subprocess.run(['xcrun', 'swiftc', *[str(p) for p in sorted((ROOT / 'Sources').glob('*.swift'))],
                        '-framework', 'Cocoa', '-framework', 'AVFoundation', '-framework', 'Security', '-o', str(executable)], check=True)
        shutil.copy2(executable, app / 'MacOS/RoxyHD.next')
        os.replace(app / 'MacOS/RoxyHD.next', app / 'MacOS/RoxyHD')
    shutil.copy2(ROOT / 'config/Info.plist', app / 'Info.plist')
    shutil.copy2(ROOT / 'assets/pet/animations.json', app / 'Resources/animations.json')
    shutil.copytree(ROOT / 'assets/pet/frames', app / 'Resources/frames', dirs_exist_ok=True)
    if (ROOT / 'assets/fonts').exists():
        shutil.copytree(ROOT / 'assets/fonts', app / 'Resources/fonts', dirs_exist_ok=True)
    (app / 'Resources/project-root.txt').write_text(str(ROOT))
    subprocess.run(['codesign', '--force', '--deep', '--sign', '-', str(app.parent)], check=True)
    print(app.parent)

if __name__ == '__main__':
    main()
