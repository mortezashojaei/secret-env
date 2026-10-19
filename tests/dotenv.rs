use std::{fs, io::ErrorKind};

use secret_env::Error;

// One test per binary: it changes the working directory and environment.
#[test]
fn dotenv_matches_dotenvy() {
    let dir = std::env::temp_dir().join(format!("secret-env-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    std::env::set_current_dir(&dir).unwrap();

    let err = secret_env::dotenv().unwrap_err();
    assert!(matches!(&err, Error::Dotenv(io) if io.kind() == ErrorKind::NotFound));

    fs::write(".env", "API_KEY='sk-leaky\n").unwrap();
    let err = secret_env::dotenv().unwrap_err();
    assert!(!format!("{err} {err:?}").contains("sk-leaky"));

    fs::write(".env", "SECRET_ENV_TEST_EXISTING=from-file\nSECRET_ENV_TEST_NEW=new\n").unwrap();
    secret_env::dotenv().unwrap();
    let read = |name| secret_env::require(name).unwrap().with_str(str::to_owned).unwrap();
    assert_eq!(read("SECRET_ENV_TEST_EXISTING"), "from-env");
    assert_eq!(read("SECRET_ENV_TEST_NEW"), "new");

    fs::remove_dir_all(&dir).unwrap();
}
