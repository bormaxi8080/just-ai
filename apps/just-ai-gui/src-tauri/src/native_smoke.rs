//! Explicit WebView2 API configuration for the opt-in Windows test binary.
use std::path::PathBuf;

pub(crate) fn configure(
  config: &mut tauri::Config,
  port: &str,
  profile: PathBuf,
) -> Result<(), String> {
  let port = port
    .parse::<std::num::NonZeroU16>()
    .map_err(|_| "JUST_AI_WEBDRIVER_PORT must be a nonzero TCP port".to_owned())?;
  if !profile.is_absolute() {
    return Err("JUST_AI_WEBDRIVER_PROFILE must be absolute".to_owned());
  }
  let window = config
    .app
    .windows
    .iter_mut()
    .find(|window| window.label == "main")
    .ok_or_else(|| "native smoke requires the main window".to_owned())?;
  // Wry 0.55.1 replaces its defaults when additional_browser_args is supplied.
  // Preserve its default feature exclusions while adding only our local port.
  window.additional_browser_args = Some(format!(
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --remote-debugging-port={port} --remote-debugging-address=127.0.0.1"
  ));
  window.data_directory = Some(profile);
  Ok(())
}

#[cfg(target_os = "windows")]
pub(crate) fn configure_from_environment(config: &mut tauri::Config) -> Result<(), String> {
  let port = std::env::var("JUST_AI_WEBDRIVER_PORT").map_err(|error| error.to_string())?;
  let profile = std::env::var_os("JUST_AI_WEBDRIVER_PROFILE")
    .ok_or_else(|| "JUST_AI_WEBDRIVER_PROFILE is required".to_owned())?;
  configure(config, &port, PathBuf::from(profile))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn config() -> tauri::Config {
    serde_json::from_str(include_str!("../tauri.conf.json")).unwrap()
  }

  #[test]
  fn supplies_api_arguments_and_profile_to_main_window() {
    let mut config = config();
    let profile = std::env::temp_dir().join("native-smoke-profile");
    configure(&mut config, "12345", profile.clone()).unwrap();
    let window = &config.app.windows[0];
    assert_eq!(window.data_directory, Some(profile));
    let args = window.additional_browser_args.as_ref().unwrap();
    assert!(args.contains("--remote-debugging-port=12345"));
    assert!(args.contains("--remote-debugging-address=127.0.0.1"));
    assert!(args.contains("--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection"));
  }

  #[test]
  fn rejects_invalid_ports_without_changing_config() {
    for port in [
      "0",
      "65536",
      "-1",
      "12345 --remote-debugging-address=0.0.0.0",
    ] {
      let mut config = config();
      assert!(configure(&mut config, port, std::env::temp_dir()).is_err());
      assert!(config.app.windows[0].additional_browser_args.is_none());
    }
  }

  #[test]
  fn rejects_relative_profiles() {
    assert!(configure(&mut config(), "12345", PathBuf::from("relative")).is_err());
  }
}
