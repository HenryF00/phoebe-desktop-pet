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
        subprocess.run(['xcrun', 'swiftc', str(ROOT / 'Sources/main.swift'),
                        '-framework', 'Cocoa', '-o', str(executable)], check=True)
        shutil.copy2(executable, app / 'MacOS/RoxyHD.next')
        os.replace(app / 'MacOS/RoxyHD.next', app / 'MacOS/RoxyHD')
    shutil.copy2(ROOT / 'config/Info.plist', app / 'Info.plist')
    shutil.copy2(ROOT / 'assets/pet/animations.json', app / 'Resources/animations.json')
    shutil.copytree(ROOT / 'assets/pet/frames', app / 'Resources/frames', dirs_exist_ok=True)
    print(app.parent)

if __name__ == '__main__':
    main()

