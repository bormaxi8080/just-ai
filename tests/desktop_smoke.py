#!/usr/bin/env python3
"""Native Tauri IPC/UI smoke for Linux/Windows, in a disposable project.

Prerequisites: Selenium, tauri-driver, platform WebDriver, built GUI and just.
Linux: run under xvfb-run. Windows: Selenium Manager resolves WebDriver for
the installed WebView2 runtime.
"""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
from urllib.error import URLError
from urllib.request import ProxyHandler, build_opener


def stop_driver(process, windows, log):
    """Stop native driver and app descendants, including failed-session launches."""
    if windows:
        # Killing only the proxy leaves EdgeDriver and its app holding cwd open.
        subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                       stdout=log, stderr=subprocess.STDOUT, check=False)
    else:
        process.terminate()
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()


def tauri_options(application, root, windows):
    options = {"application": str(application)}
    if windows:
        # Keep EdgeDriver's DevToolsActivePort lookup and WebView2 on one profile.
        options["webviewOptions"] = {"userDataFolder": str(root / "webview2")}
    return options


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--application", required=True, type=Path)
    parser.add_argument("--just", required=True, type=Path)
    parser.add_argument("--artifacts", type=Path, default=Path("desktop-test-artifacts"))
    args = parser.parse_args()
    if sys.platform == "darwin":
        parser.error("Native tauri-driver does not support WKWebView on macOS")
    from selenium import webdriver
    from selenium.webdriver.common.by import By
    from selenium.webdriver.common.options import ArgOptions
    from selenium.webdriver.support.ui import WebDriverWait
    from selenium.webdriver.common.selenium_manager import SeleniumManager

    class Options(ArgOptions):
        @property
        def default_capabilities(self):
            return {"browserName": "wry"}

    app = args.application.resolve(strict=True)
    runner = args.just.resolve(strict=True)
    python = Path(sys.executable).as_posix()
    args.artifacts.mkdir(parents=True, exist_ok=True)
    artifacts = args.artifacts.resolve()
    command = ["tauri-driver", "--port", "4444", "--native-port", "4445"]
    if os.name == "nt":
        native = SeleniumManager().binary_paths(["--browser", "webview2"])["driver_path"]
        command += ["--native-driver", native]
    with tempfile.TemporaryDirectory(prefix="just-ai-desktop-") as temporary:
        root = Path(temporary)
        (root / "justfile").write_text(
            f'hello:\n    @echo native-desktop-ok\n\nquiet:\n    @"{python}" -c "import time; print(\'native-running\', flush=True); time.sleep(60)"\n',
            encoding="utf-8",
        )
        config = root / "just-ai.toml"
        config.write_text("[execution]\njust_binary = " + json.dumps(str(runner)) + "\nread_timeout_secs = 0\n", encoding="utf-8")
        environment = dict(os.environ, JUST_AI_DATA_DIR=str(root / "data"))
        driver = None
        with (artifacts / "driver.log").open("w", encoding="utf-8") as log:
            process = subprocess.Popen(command, cwd=root, env=environment, stdout=log, stderr=subprocess.STDOUT)
            try:
                deadline = time.monotonic() + 30
                while True:
                    if process.poll() is not None:
                        raise RuntimeError("tauri-driver exited; see driver.log")
                    try:
                        with socket.create_connection(("127.0.0.1", 4444), timeout=1):
                            pass
                        # The proxy starts before its native driver accepts sessions.
                        # Probe native readiness, bypassing ambient HTTP proxies.
                        with build_opener(ProxyHandler({})).open("http://127.0.0.1:4445/status", timeout=1) as response:
                            status = json.load(response)
                        if status.get("value", {}).get("ready"):
                            break
                    except (OSError, URLError, ValueError):
                        pass
                    if time.monotonic() >= deadline:
                        raise TimeoutError("tauri-driver/native WebDriver did not become ready; see driver.log")
                    time.sleep(0.1)
                options = Options()
                options.set_capability("tauri:options", tauri_options(app, root, os.name == "nt"))
                driver = webdriver.Remote("http://127.0.0.1:4444", options=options)
                wait = WebDriverWait(driver, 30)
                def button(text):
                    return wait.until(lambda d: d.find_element(By.XPATH, f"//button[normalize-space(.)='{text}']"))
                def contains(text):
                    wait.until(lambda d: text in d.find_element(By.TAG_NAME, "body").text)
                button("Prepare & run").click()
                wait.until(lambda d: "native-desktop-ok" in d.find_element(By.CSS_SELECTOR, ".run-output pre").text)
                wait.until(lambda d: d.find_elements(By.CSS_SELECTOR, ".history-row"))
                # Configure a deny policy, re-inspect and prove execution is refused.
                with config.open("a", encoding="utf-8") as stream:
                    stream.write('\n[policy.decisions.low]\ntype = "deny"\nreason = "native-policy-denied"\n')
                button("Inspect project").click()
                button("Prepare & run").click()
                contains("native-policy-denied")
                assert len(driver.find_elements(By.CSS_SELECTOR, ".history-row")) == 1
                # Return to allow, select a long run, observe native events and cancel.
                config.write_text("[execution]\njust_binary = " + json.dumps(str(runner)) + "\nread_timeout_secs = 0\n", encoding="utf-8")
                button("Inspect project").click()
                wait.until(lambda d: d.find_element(By.CSS_SELECTOR, "button.recipe").is_displayed())
                driver.find_element(By.XPATH, "//button[contains(@class,'recipe')][.//*[normalize-space(.)='quiet']]").click()
                button("Prepare & run").click()
                wait.until(lambda d: "native-running" in d.find_element(By.CSS_SELECTOR, ".run-output pre").text)
                button("Cancel run").click()
                contains("cancelled")
                wait.until(lambda d: not d.find_elements(By.CSS_SELECTOR, ".cancel-button"))
                print("Native desktop inspect/run/history/deny/events/cancel smoke passed")
            except Exception:
                if driver:
                    driver.save_screenshot(str(artifacts / "failure.png"))
                    (artifacts / "failure.html").write_text(driver.page_source, encoding="utf-8")
                raise
            finally:
                try:
                    if driver:
                        driver.quit()
                finally:
                    stop_driver(process, os.name == "nt", log)


if __name__ == "__main__":
    main()
