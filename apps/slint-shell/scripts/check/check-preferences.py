import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

root = Path(__file__).resolve().parents[2]
binary = Path(os.environ.get('TECLOID_BIN', root.parents[1] / 'target/debug/tecloid')).resolve()
output = Path(sys.argv[1] if len(sys.argv) > 1 else tempfile.mkdtemp(prefix='tecloid-3b-ipc-'))
output.mkdir(parents=True, exist_ok=True)
config = tempfile.TemporaryDirectory(prefix="tecloid-preferences-config-")
config_path = Path(config.name) / "linux/.config/dev.bozonx.tecloid/config.json"
config_path.parent.mkdir(parents=True)
config_path.write_text('{"version":1,"settings":{"experimental":true}}', encoding="utf-8")
env = dict(
    os.environ,
    TECLOID_DEV_DIR=config.name,
    TECLOID_SOCKET=f'tecloid-3b-{os.getpid()}.sock',
    TECLOID_METRICS=str(output / 'parent.csv'),
    TECLOID_POPUPS='spell',
    TECLOID_HOTKEYS='off',
    SLINT_BACKEND='winit-skia-software',
    RUST_LOG='info',
)
socket = Path(env['XDG_RUNTIME_DIR']) / 'tecloid-slint-shell' / env['TECLOID_SOCKET']


def send(command):
    subprocess.run([binary, *command.split()], env=env, check=True, capture_output=True, timeout=4)


with (output / 'preferences.log').open('w') as log:
    process = subprocess.Popen([binary], env=env, stdout=log, stderr=log)
    try:
        for _ in range(100):
            if process.poll() is not None:
                raise RuntimeError(f'shell exited during startup; see {output}')
            if socket.exists():
                result = subprocess.run([binary, 'ping'], env=env, capture_output=True, timeout=4)
                if result.returncode == 0:
                    break
            time.sleep(.1)
        else:
            raise RuntimeError(f'startup timeout; see {output}')
        for command in [
            'preferences light en', 'show emoji', 'preferences dark ru', 'hide',
            'preferences light en', 'show quick', 'preferences dark ru', 'hide',
            'preferences light en', 'show emoji', 'preferences dark es',
            'preferences light zh-Hant', 'hide', 'show quick', 'preferences dark ja', 'hide',
        ]:
            send(command)
            time.sleep(.4)
        send('quit')
        process.wait(timeout=5)
        assert process.returncode == 0
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        config.cleanup()

lines = (output / 'preferences.log').read_text()
for expected in [
    'dark=false, language=en, visible=None',
    'dark=true, language=ru, visible=Some("emoji")',
    'dark=true, language=ru, visible=Some("quick")',
    'dark=true, language=es, visible=Some("emoji")',
    'dark=false, language=zh-Hant, visible=Some("emoji")',
    'dark=true, language=ja, visible=Some("quick")',
]:
    assert expected in lines, expected
assert 'Spell preferences:' not in lines

hint_config = tempfile.TemporaryDirectory(prefix="tecloid-preferences-hint-config-")
hint_config_path = Path(hint_config.name) / "linux/.config/dev.bozonx.tecloid/config.json"
hint_config_path.parent.mkdir(parents=True)
hint_config_path.write_text(
    '{"version":1,"settings":{"experimental":true,"interface":{"layerHint":true}}}',
    encoding="utf-8",
)
hint_env = dict(
    env,
    TECLOID_DEV_DIR=hint_config.name,
    TECLOID_SOCKET=f'tecloid-3b-hint-{os.getpid()}.sock',
    TECLOID_METRICS=str(output / 'hint-parent.csv'),
    TECLOID_POPUPS='winit',
)
hint_socket = Path(hint_env['XDG_RUNTIME_DIR']) / 'tecloid-slint-shell' / hint_env['TECLOID_SOCKET']
hint_log_path = output / 'preferences-hint.log'


def send_hint(command):
    subprocess.run([binary, *command.split()], env=hint_env, check=True, capture_output=True, timeout=4)


with hint_log_path.open('w') as log:
    process = subprocess.Popen([binary], env=hint_env, stdout=log, stderr=log)
    try:
        for _ in range(100):
            if process.poll() is not None:
                raise RuntimeError(f'shell exited during hint startup; see {output}')
            if (
                hint_socket.exists()
                and 'Spell ready:' in hint_log_path.read_text()
                and subprocess.run([binary, 'ping'], env=hint_env, capture_output=True, timeout=4).returncode == 0
            ):
                break
            time.sleep(.1)
        else:
            raise RuntimeError(f'hint worker startup timeout; see {output}')
        for command in ['preferences light en', 'preferences dark ru', 'preferences light es']:
            send_hint(command)
            time.sleep(.4)
        send_hint('quit')
        process.wait(timeout=5)
        assert process.returncode == 0
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        hint_config.cleanup()

hint_lines = hint_log_path.read_text()
for expected in [
    'dark=false, language=en, visible=None',
    'dark=true, language=ru, visible=None',
    'dark=false, language=es, visible=None',
]:
    assert expected in hint_lines, expected
assert 'Spell preferences:' not in hint_lines
print(f'Spell preferences IPC: passed (hidden, emoji, quick, winit layer-hint, clean shutdown). Logs: {output}')
