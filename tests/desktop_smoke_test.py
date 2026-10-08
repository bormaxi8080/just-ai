"""Portable regression checks for native desktop session setup and teardown."""
import io
from pathlib import Path
import subprocess
import unittest
from unittest.mock import Mock, patch

from desktop_smoke import stop_driver, tauri_options


class DesktopSmokeTests(unittest.TestCase):
    def test_windows_profile_is_explicit(self):
        options = tauri_options(Path('app.exe'), Path('project'), True)
        self.assertEqual(options['webviewOptions']['userDataFolder'], str(Path('project/webview2')))

    def test_linux_capabilities_have_no_edge_options(self):
        self.assertEqual(tauri_options(Path('app'), Path('project'), False), {'application': 'app'})

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
