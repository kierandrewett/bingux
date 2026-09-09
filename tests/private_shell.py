"""Run a private shell until its QML assertions write their completion report."""
import os
from pathlib import Path
import resource
import shutil
import signal
import subprocess
import time


def stage_compositor_bridge(repo, config):
    """Stage the current bridge before the private input service reloads scripts."""
    config = Path(config).resolve()
    if not str(config).startswith('/tmp/gnoblin-gs.') or config.name != 'config' or not os.environ.get('WAYLAND_DISPLAY', '').startswith('gnoblin-gs-'):
        raise RuntimeError('Only a private Gnoblin session can receive the test bridge')
    scripts = config / 'gnoblin/scripts'
    scripts.mkdir(parents=True, exist_ok=True)
    source = Path(repo).parent / 'gnoblin/src/scripts'
    shutil.copy2(source / 'compositor-bridge.js', scripts)
    shutil.copytree(source / 'lib', scripts / 'lib', dirs_exist_ok=True)
    return scripts


def run_reported_shell(fixture, environment, report_variable, timeout=30, complete=lambda text: bool(text)):
    # Failed UI tests must not leave full memory dumps in the checkout.
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    environment = environment | {report_variable: str(fixture / 'report')}
    with (fixture / 'runtime.log').open('w') as log:
        process = subprocess.Popen([os.environ.get('QS_TEST_BIN', 'qs'), '-p', str(fixture), '--no-color'],
            env=environment, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            deadline = time.monotonic() + timeout
            report = ''
            while time.monotonic() < deadline and process.poll() is None:
                if (fixture / 'report').exists():
                    report = (fixture / 'report').read_text()
                    if complete(report):
                        break
                time.sleep(.05)
            if not complete(report):
                exit_code = process.poll()
                report = ('Timed out before completion' if exit_code is None else
                          f'Shell exited before completion (exit status {exit_code})') + '\n' + report
        finally:
            # End only this fixture's asynchronous services after its assertions.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait(timeout=5)
    return report, (fixture / 'runtime.log').read_text()
