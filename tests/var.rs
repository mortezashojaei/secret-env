use secret_env::{EnvSecret, Error};

#[test]
fn require_reads_value() {
    let mut secret = secret_env::require("SECRET_ENV_TEST_KEY").unwrap();
    assert_eq!(secret.with_str(str::to_owned).unwrap(), "sk-test-123");
    assert_eq!(secret.with(|bytes| bytes.len()).unwrap(), 11);
}

#[test]
fn missing_var() {
    assert!(secret_env::var("SECRET_ENV_TEST_MISSING").unwrap().is_none());
    let err = secret_env::require("SECRET_ENV_TEST_MISSING").unwrap_err();
    assert!(matches!(err, Error::NotPresent(name) if name == "SECRET_ENV_TEST_MISSING"));
}

#[test]
fn too_long() {
    let err = secret_env::var_sized::<4>("SECRET_ENV_TEST_KEY").unwrap_err();
    assert!(matches!(err, Error::TooLong { len: 11, max: 4, .. }));
    assert!(!format!("{err} {err:?}").contains("sk-test"));
}

#[test]
fn sized_fits_exactly() {
    let mut secret: EnvSecret<11> = secret_env::require_sized("SECRET_ENV_TEST_KEY").unwrap();
    assert_eq!(secret.with_str(str::to_owned).unwrap(), "sk-test-123");
}

#[test]
fn debug_prints_length_only() {
    let secret = secret_env::require("SECRET_ENV_TEST_KEY").unwrap();
    assert_eq!(format!("{secret:?}"), "EnvSecret { len: 11 }");
}

#[test]
fn leaves_environment_alone() {
    secret_env::require("SECRET_ENV_TEST_KEY").unwrap();
    assert_eq!(std::env::var("SECRET_ENV_TEST_KEY").unwrap(), "sk-test-123");
}
