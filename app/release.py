"""Build a signed release and publish it to the in-app updater.

    python release.py "release notes (Uzbek, shown in SOZLAMALAR)"

1. builds the NSIS installer with the updater signature (the private key lives OUTSIDE the repo:
   %KR_UPDATER_KEY% or C:\\Users\\soka\\kawaii-keys\\desk-updater.key — never commit it);
2. copies it to ../dist/KawaiiRadio-<version>-setup.exe;
3. writes ../dist/latest.json, which every installed copy reads from GitHub (raw, main branch) — so the update goes
   live when the commit with dist/ is pushed.
"""
import datetime, json, os, shutil, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
DIST = os.path.join(HERE, '..', 'dist')
RAW = 'https://raw.githubusercontent.com/soka8imokenp/kawaii_radio_desktop/main/dist/'
KEY = os.environ.get('KR_UPDATER_KEY', r'C:\Users\soka\kawaii-keys\desk-updater.key')

notes = sys.argv[1] if len(sys.argv) > 1 else ''
conf = json.load(open(os.path.join(HERE, 'tauri.conf.json'), encoding='utf-8'))
ver = conf['version']

env = dict(os.environ)
env['TAURI_SIGNING_PRIVATE_KEY'] = open(KEY, encoding='utf-8').read().strip()
env['TAURI_SIGNING_PRIVATE_KEY_PASSWORD'] = env.get('TAURI_SIGNING_PRIVATE_KEY_PASSWORD', '')
subprocess.run(['cargo', 'tauri', 'build', '--bundles', 'nsis'], cwd=HERE, env=env, check=True, shell=os.name == 'nt')

nsis = os.path.join(HERE, 'target', 'release', 'bundle', 'nsis')
exe = os.path.join(nsis, f'Kawaii Radio_{ver}_x64-setup.exe')
sig = open(exe + '.sig', encoding='utf-8').read().strip()
name = f'KawaiiRadio-{ver}-setup.exe'
shutil.copyfile(exe, os.path.join(DIST, name))
latest = {
    'version': ver,
    'notes': notes,
    'pub_date': datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'),
    'platforms': {'windows-x86_64': {'signature': sig, 'url': RAW + name}},
}
json.dump(latest, open(os.path.join(DIST, 'latest.json'), 'w', encoding='utf-8'), ensure_ascii=False, indent=2)
shutil.copyfile(os.path.join(HERE, 'target', 'release', 'kawaii-radio.exe'), os.path.join(HERE, '..', 'Kawaii Radio', 'Kawaii Radio.exe'))
print('RELEASE', ver, name)
