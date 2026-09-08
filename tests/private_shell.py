"""Run a private shell until its QML assertions write their completion report."""
import os
import resource
import signal
import subprocess
import time


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
                report = 'Timed out before completion\n' + report
        finally:
            # End only this fixture's asynchronous services after its assertions.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait(timeout=5)
    return report, (fixture / 'runtime.log').read_text()
