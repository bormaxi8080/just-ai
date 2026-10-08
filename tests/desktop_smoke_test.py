"""Portable regression checks for native desktop session setup and teardown."""
import io
from pathlib import Path
import subprocess
import unittest
from unittest.mock import Mock, patch

from desktop_smoke import stop_driver, windows_environment, wait_for_webview


class DesktopSmokeTests(unittest.TestCase):
    def test_windows_app_receives_debug_port_and_disposable_profile(self):
        original = {'JUST_AI_DATA_DIR': 'data', 'WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS': 'old'}
        environment = windows_environment(original, Path('project'), 12345)
        self.assertEqual(environment['WEBVIEW2_USER_DATA_FOLDER'], str(Path('project/webview2')))
        self.assertEqual(environment['WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS'],
                         '--remote-debugging-port=12345 --remote-debugging-address=127.0.0.1')
        self.assertEqual(environment['JUST_AI_DATA_DIR'], 'data')
        self.assertEqual(original['WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS'], 'old')

    def test_app_exit_reports_application_log(self):
        process = Mock(returncode=7)
        process.poll.return_value = 7
        with self.assertRaisesRegex(RuntimeError, r'exited \(7\).*application.log'):
            wait_for_webview(process, 12345)

    @patch('desktop_smoke.build_opener')
    def test_webview_wait_checks_actual_devtools_endpoint(self, build_opener):
        process = Mock()
        process.poll.return_value = None
        response = build_opener.return_value.open.return_value.__enter__.return_value
        response.read.return_value = b'{"webSocketDebuggerUrl":"ws://127.0.0.1:12345/devtools/browser/test"}'
        wait_for_webview(process, 12345)
        build_opener.return_value.open.assert_called_once_with(
            'http://127.0.0.1:12345/json/version', timeout=1)

    @patch('desktop_smoke.time.monotonic', side_effect=[0, 31])
    @patch('desktop_smoke.build_opener')
    def test_incomplete_devtools_response_does_not_count_as_ready(self, build_opener, clock):
        process = Mock()
        process.poll.return_value = None
        response = build_opener.return_value.open.return_value.__enter__.return_value
        response.read.return_value = b'{}'
        with self.assertRaisesRegex(TimeoutError, 'application.log'):
            wait_for_webview(process, 12345)

    @patch('desktop_smoke.subprocess.run')
    def test_windows_failed_session_stops_entire_tree(self, run):
        process = Mock(pid=123)
        log = io.StringIO()
        stop_driver(process, True, log)
        run.assert_called_once_with(['taskkill', '/PID', '123', '/T', '/F'],
                                    stdout=log, stderr=subprocess.STDOUT, check=False)
        process.terminate.assert_not_called()
        process.wait.assert_called_once_with(timeout=10)

    def test_linux_escalates_when_proxy_does_not_exit(self):
        process = Mock()
        process.wait.side_effect = [subprocess.TimeoutExpired('tauri-driver', 10), None]
        stop_driver(process, False, io.StringIO())
        process.terminate.assert_called_once()
        process.kill.assert_called_once()
        self.assertEqual(process.wait.call_count, 2)


if __name__ == '__main__':
    unittest.main()
