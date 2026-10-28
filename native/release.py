"""Build and publish a release of the native app (Windows).

    python release.py "release notes (Uzbek, shown in the app)"

1. builds native/ (release), the version is native/Cargo.toml's;
2. makes the NSIS installer dist/KawaiiRadio-<version>-setup.exe (installer.nsi);
3. signs it with the updater key (minisign via `cargo tauri signer sign`; the key lives OUTSIDE the repo:
   %KR_UPDATER_KEY% or C:\\Users\\soka\\kawaii-keys\\desk-updater.key — never commit it);
4. writes dist/latest.json (the Linux entries made by release_linux.sh are kept). Every installed copy, the old
   Tauri version included, reads it from GitHub — the update goes live when the commit with dist/ is pushed.
"""
import datetime, json, os, re, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, '..')
DIST = os.path.join(ROOT, 'dist')
RAW = 'https://raw.githubusercontent.com/soka8imokenp/kawaii_radio_desktop/main/dist/'
KEY = os.environ.get('KR_UPDATER_KEY', r'C:\Users\soka\kawaii-keys\desk-updater.key')
TOOLS = os.path.join(ROOT, '.tools')
MAKENSIS = os.environ.get('MAKENSIS', os.path.join(os.environ.get('LOCALAPPDATA', ''), 'tauri', 'NSIS', 'makensis.exe'))

notes = sys.argv[1] if len(sys.argv) > 1 else ''
ver = re.search(r'^version = "([^"]+)"', open(os.path.join(HERE, 'Cargo.toml'), encoding='utf-8').read(), re.M).group(1)

env = dict(os.environ)
env['RUSTUP_HOME'] = os.path.join(TOOLS, 'rustup')
env['CARGO_HOME'] = os.path.join(TOOLS, 'cargo')
env['PATH'] = os.pathsep.join([os.path.join(TOOLS, 'cargo', 'bin'), os.path.join(TOOLS, 'mingw64', 'bin'), env['PATH']])
subprocess.run(['taskkill', '/F', '/IM', 'kawaii-radio.exe'], capture_output=True)
subprocess.run(['cargo', 'build', '--release'], cwd=HERE, env=env, check=True, shell=os.name == 'nt')
subprocess.run([MAKENSIS, f'/DVERSION={ver}', 'installer.nsi'], cwd=HERE, check=True)

name = f'KawaiiRadio-{ver}-setup.exe'
exe = os.path.join(DIST, name)
env['TAURI_SIGNING_PRIVATE_KEY'] = open(KEY, encoding='utf-8').read().strip()
env['TAURI_SIGNING_PRIVATE_KEY_PASSWORD'] = env.get('TAURI_SIGNING_PRIVATE_KEY_PASSWORD', '')
subprocess.run(['cargo', 'tauri', 'signer', 'sign', exe], cwd=os.path.join(ROOT, 'app'), env=env, check=True, shell=os.name == 'nt')
sig = open(exe + '.sig', encoding='utf-8').read().strip()
os.remove(exe + '.sig')

path = os.path.join(DIST, 'latest.json')
latest = json.load(open(path, encoding='utf-8')) if os.path.exists(path) else {'platforms': {}}
latest['version'] = ver
latest['notes'] = notes
latest['pub_date'] = datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
latest['platforms']['windows-x86_64'] = {'signature': sig, 'url': RAW + name}
# a platform still pointing at an older version would make its copies install that one as "the new version": dropped
latest['platforms'] = {k: v for k, v in latest['platforms'].items() if k == 'windows-x86_64' or f'-{ver}' in v['url'] or f'_{ver}' in v['url']}
json.dump(latest, open(path, 'w', encoding='utf-8'), ensure_ascii=False, indent=2)
print('RELEASE', ver, name)
