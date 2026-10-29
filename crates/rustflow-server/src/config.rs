//! Environment-backed configuration for a RustFlow server application.
//!
//! Development and test profiles may read dotenv-style files from the current
//! working directory. Production deliberately ignores all dotenv files and
//! requires its bind host and port to come from the process environment.

use std::{
    collections::BTreeMap,
    env,
    error::Error,
    fmt, fs,
    net::{IpAddr, SocketAddr},
    path::Path,
};

/// The supported application runtime profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEnvironment {
    /// Local development. Defaults bind only to loopback.
    Development,
    /// Test execution, with settings optionally loaded from .env.test.
    Test,
    /// Production. Dotenv files are ignored and bind settings are required explicitly.
    Production,
}

impl AppEnvironment {
    fn parse(value: &str) -> Result<Self, ConfigError> {
        match value {
            "development" => Ok(Self::Development),
            "test" => Ok(Self::Test),
            "production" => Ok(Self::Production),
            _ => Err(ConfigError(
                "RUSTFLOW_ENV must be development, test, or production".to_owned(),
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Test => "test",
            Self::Production => "production",
        }
    }
}

/// Validated settings needed to bind a RustFlow HTTP application.
///
/// This intentionally does not contain credentials or secret values. Additional
/// settings should be introduced only when a real framework capability needs them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    /// Selected application profile.
    pub environment: AppEnvironment,
    /// Numeric IPv4 or IPv6 address to bind. Host names are not accepted.
    pub host: IpAddr,
    /// TCP port in the range 1..=65535.
    pub port: u16,
}

impl AppConfig {
    /// Load and validate configuration from the process environment and, in
    /// development/test only, dotenv-style files in the current directory.
    ///
    /// Precedence is process environment, profile-specific file, base .env, then
    /// defaults. Production ignores dotenv files and requires bind settings in
    /// the process environment.
    pub fn load() -> Result<Self, ConfigError> {
        let process_environment = env::vars().collect::<BTreeMap<_, _>>();
        let working_directory = env::current_dir().map_err(|error| {
            ConfigError(format!("could not determine current directory: {error}"))
        })?;
        Self::from_sources(&process_environment, &working_directory)
    }

    /// Return the socket address validated by this configuration.
    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }

    /// Resolve one non-secret setting using the same precedence as AppConfig.
    ///
    /// In development/test, values are read from `.env`, then `.env.<profile>`, then the
    /// process environment. In production, only the process environment is consulted.
    /// Callers must not log returned values because settings may contain credentials.
    pub fn setting(&self, key: &str) -> Result<Option<String>, ConfigError> {
        if !valid_env_key(key) {
            return Err(ConfigError("invalid setting name".to_owned()));
        }

        let process_environment = env::vars().collect::<BTreeMap<_, _>>();
        if self.environment == AppEnvironment::Production {
            return Ok(process_environment.get(key).cloned());
        }

        let working_directory = env::current_dir().map_err(|error| {
            ConfigError(format!("could not determine current directory: {error}"))
        })?;
        let mut values = read_env_file(&working_directory.join(".env"))?;
        values.extend(read_env_file(
            &working_directory.join(format!(".env.{}", self.environment.as_str())),
        )?);
        values.extend(process_environment);
        Ok(values.get(key).cloned())
    }

    fn from_sources(
        process_environment: &BTreeMap<String, String>,
        working_directory: &Path,
    ) -> Result<Self, ConfigError> {
        if let Some(profile) = process_environment.get("RUSTFLOW_ENV") {
            let environment = AppEnvironment::parse(profile)?;
            if environment == AppEnvironment::Production {
                return Self::from_production_environment(process_environment);
            }

            let base = read_env_file(&working_directory.join(".env"))?;
            return Self::from_nonproduction_sources(
                environment,
                base,
                process_environment,
                working_directory,
            );
        }

        let base = read_env_file(&working_directory.join(".env"))?;
        let profile = base
            .get("RUSTFLOW_ENV")
            .map(String::as_str)
            .unwrap_or("development");
        let environment = AppEnvironment::parse(profile)?;
        if environment == AppEnvironment::Production {
            return Err(ConfigError(
                "production must be selected through the process environment; dotenv files are disabled for production"
                    .to_owned(),
            ));
        }

        Self::from_nonproduction_sources(environment, base, process_environment, working_directory)
    }

    fn from_production_environment(
        process_environment: &BTreeMap<String, String>,
    ) -> Result<Self, ConfigError> {
        let host = parse_host(process_environment.get("RUSTFLOW_HOST"), true)?;
        let port = parse_port(process_environment.get("RUSTFLOW_PORT"), true)?;
        Ok(Self {
            environment: AppEnvironment::Production,
            host,
            port,
        })
    }

    fn from_nonproduction_sources(
        environment: AppEnvironment,
        mut values: BTreeMap<String, String>,
        process_environment: &BTreeMap<String, String>,
        working_directory: &Path,
    ) -> Result<Self, ConfigError> {
        let profile_values =
            read_env_file(&working_directory.join(format!(".env.{}", environment.as_str())))?;
        values.extend(profile_values);
        values.extend(
            process_environment
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        values.insert("RUSTFLOW_ENV".to_owned(), environment.as_str().to_owned());

        Ok(Self {
            environment,
            host: parse_host(values.get("RUSTFLOW_HOST"), false)?,
            port: parse_port(values.get("RUSTFLOW_PORT"), false)?,
        })
    }
}

/// An actionable configuration error that never includes configured values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(String);

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ConfigError {}

fn parse_host(value: Option<&String>, required: bool) -> Result<IpAddr, ConfigError> {
    let value = match value {
        Some(value) => value,
        None if required => {
            return Err(ConfigError(
                "RUSTFLOW_HOST must be set in the process environment for production".to_owned(),
            ));
        }
        None => return Ok(IpAddr::from([127, 0, 0, 1])),
    };

    value
        .parse::<IpAddr>()
        .map_err(|_| ConfigError("RUSTFLOW_HOST must be a numeric IPv4 or IPv6 address".to_owned()))
}

fn parse_port(value: Option<&String>, required: bool) -> Result<u16, ConfigError> {
    let port = match value {
        Some(value) => value.parse::<u16>().map_err(|_| {
            ConfigError("RUSTFLOW_PORT must be an integer from 1 to 65535".to_owned())
        })?,
        None if required => {
            return Err(ConfigError(
                "RUSTFLOW_PORT must be set in the process environment for production".to_owned(),
            ));
        }
        None => 3000,
    };

    if port == 0 {
        return Err(ConfigError(
            "RUSTFLOW_PORT must be an integer from 1 to 65535".to_owned(),
        ));
    }
    Ok(port)
}

fn read_env_file(path: &Path) -> Result<BTreeMap<String, String>, ConfigError> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BTreeMap::new());
        }
        Err(error) => {
            return Err(ConfigError(format!(
                "could not read configuration file {}: {error}",
                path.display()
            )));
        }
    };

    parse_env_file(&contents, path)
}

fn parse_env_file(contents: &str, path: &Path) -> Result<BTreeMap<String, String>, ConfigError> {
    let mut values = BTreeMap::new();
    for (index, raw_line) in contents.lines().enumerate() {
        let mut line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(exported) = line.strip_prefix("export ") {
            line = exported.trim_start();
        }

        let Some((raw_key, raw_value)) = line.split_once('=') else {
            return Err(ConfigError(format!(
                "invalid configuration syntax at {}:{}",
                path.display(),
                index + 1
            )));
        };
        let key = raw_key.trim();
        if !valid_env_key(key) {
            return Err(ConfigError(format!(
                "invalid environment variable name at {}:{}",
                path.display(),
                index + 1
            )));
        }

        let value = parse_env_value(raw_value).ok_or_else(|| {
            ConfigError(format!(
                "invalid quoted value at {}:{}",
                path.display(),
                index + 1
            ))
        })?;
        values.insert(key.to_owned(), value);
    }
    Ok(values)
}

fn valid_env_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn parse_env_value(raw_value: &str) -> Option<String> {
    let value = raw_value.trim();
    if value.is_empty() {
        return Some(String::new());
    }

    let first = value.as_bytes()[0];
    if first == b'"' || first == 0x27 {
        if value.len() < 2 || value.as_bytes().last() != Some(&first) {
            return None;
        }
        return Some(value[1..value.len() - 1].to_owned());
    }

    let value = value
        .split_once(" #")
        .map(|(value, _comment)| value)
        .unwrap_or(value)
        .trim_end();
    Some(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        net::{IpAddr, Ipv4Addr, Ipv6Addr},
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(1);

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = env::temp_dir().join(format!(
                "rustflow-config-test-{}-{}",
                std::process::id(),
                NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn write(&self, name: &str, contents: &str) {
            fs::write(self.0.join(name), contents).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn resolve(directory: &TempDir, vars: &[(&str, &str)]) -> Result<AppConfig, ConfigError> {
        let process_environment = vars
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect::<BTreeMap<_, _>>();
        AppConfig::from_sources(&process_environment, &directory.0)
    }

    #[test]
    fn defaults_to_development_loopback_and_port_3000() {
        let directory = TempDir::new();
        let config = resolve(&directory, &[]).unwrap();

        assert_eq!(config.environment, AppEnvironment::Development);
        assert_eq!(config.host, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(config.port, 3000);
        assert_eq!(config.socket_addr().port(), 3000);
    }

    #[test]
    fn process_environment_overrides_profile_and_base_dotenv_values() {
        let directory = TempDir::new();
        directory.write(
            ".env",
            "RUSTFLOW_ENV=development\nRUSTFLOW_HOST=127.0.0.1\nRUSTFLOW_PORT=3100\n",
        );
        directory.write(
            ".env.development",
            "RUSTFLOW_HOST=0.0.0.0\nRUSTFLOW_PORT=3200\n",
        );

        let config = resolve(
            &directory,
            &[("RUSTFLOW_HOST", "::1"), ("RUSTFLOW_PORT", "3300")],
        )
        .unwrap();

        assert_eq!(config.host, IpAddr::V6(Ipv6Addr::LOCALHOST));
        assert_eq!(config.port, 3300);
    }

    #[test]
    fn test_profile_overrides_base_dotenv_values() {
        let directory = TempDir::new();
        directory.write(".env", "RUSTFLOW_ENV=test\nRUSTFLOW_PORT=3100\n");
        directory.write(".env.test", "RUSTFLOW_PORT=3200\n");

        let config = resolve(&directory, &[]).unwrap();

        assert_eq!(config.environment, AppEnvironment::Test);
        assert_eq!(config.port, 3200);
    }

    #[test]
    fn production_requires_process_host_and_port_and_never_reads_dotenv() {
        let directory = TempDir::new();
        directory.write(".env", "this file is intentionally not valid dotenv\n");

        let missing_host = resolve(&directory, &[("RUSTFLOW_ENV", "production")]).unwrap_err();
        assert!(missing_host.to_string().contains("RUSTFLOW_HOST"));

        let config = resolve(
            &directory,
            &[
                ("RUSTFLOW_ENV", "production"),
                ("RUSTFLOW_HOST", "0.0.0.0"),
                ("RUSTFLOW_PORT", "8080"),
            ],
        )
        .unwrap();
        assert_eq!(config.environment, AppEnvironment::Production);
        assert_eq!(config.host, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn dotenv_cannot_select_production_without_process_environment() {
        let directory = TempDir::new();
        directory.write(
            ".env",
            "RUSTFLOW_ENV=production\nRUSTFLOW_HOST=0.0.0.0\nRUSTFLOW_PORT=8080\n",
        );

        let error = resolve(&directory, &[]).unwrap_err();
        assert!(error.to_string().contains("process environment"));
    }

    #[test]
    fn invalid_profile_host_port_and_dotenv_syntax_fail_closed() {
        let directory = TempDir::new();
        assert!(resolve(&directory, &[("RUSTFLOW_ENV", "prod")]).is_err());
        assert!(resolve(&directory, &[("RUSTFLOW_HOST", "localhost")]).is_err());
        assert!(resolve(&directory, &[("RUSTFLOW_PORT", "70000")]).is_err());
        assert!(resolve(&directory, &[("RUSTFLOW_PORT", "0")]).is_err());

        directory.write(".env", "not a valid line\n");
        let error = resolve(&directory, &[]).unwrap_err();
        assert!(error.to_string().contains(".env:1"));
    }

    #[test]
    fn dotenv_parser_accepts_comments_exports_and_quotes_without_interpolation() {
        let directory = TempDir::new();
        directory.write(
            ".env",
            "# comment\nexport RUSTFLOW_ENV=development\nRUSTFLOW_PORT=\"3210\"\n",
        );

        let config = resolve(&directory, &[]).unwrap();
        assert_eq!(config.port, 3210);
    }
}
