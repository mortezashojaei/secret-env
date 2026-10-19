# secret-env

Read environment variables straight into [`memsafe::Secret`](https://docs.rs/memsafe):
locked, dump-excluded memory that cannot be printed, cloned or dereferenced.

```sh
cargo add secret-env
```

The crate is `secret-env` and you import it as `secret_env`. The crates.io name
`secretenv` belongs to an unrelated crate.

```rust
secret_env::dotenv().ok();

let mut api_key = secret_env::require("API_KEY")?;
println!("{api_key:?}"); // EnvSecret { len: 40 }
let is_live = api_key.with_str(|key| key.starts_with("sk-live-"))?;
```

## API

| Item | Returns |
| --- | --- |
| `dotenv()` | `Result<PathBuf, Error>` |
| `var(name)` | `Result<Option<EnvSecret>, Error>` |
| `require(name)` | `Result<EnvSecret, Error>` |
| `var_sized::<N>(name)` | `Result<Option<EnvSecret<N>>, Error>` |
| `require_sized::<N>(name)` | `Result<EnvSecret<N>, Error>` |
| `EnvSecret::with(f)` | `Result<R, Error>`, with `f: FnOnce(&[u8]) -> R` |
| `EnvSecret::with_str(f)` | `Result<R, Error>`, with `f: FnOnce(&str) -> R` |

`var` returns `Ok(None)` when the variable is unset. `require` returns
`Error::NotPresent` instead. `EnvSecret` defaults to 2048 bytes, so `require`
returns `EnvSecret<2048>`. Use the `_sized` versions for larger values. A value
longer than `N` returns `Error::TooLong`, and no `Secret` is allocated.

`EnvSecret` implements `Debug`, which prints only the length. It has no
`Display`, `Deref` or `Clone`, and it does not use `MemSafe<String>`, which
would protect only the string header and leave the bytes on the heap. Errors
contain variable names and lengths, never values.

## Examples

`with` and `with_str` are synchronous. Build what you need inside the closure,
then `.await` after it returns. On Unix, the page is readable only while the
closure runs.

### `OPENAI_API_KEY` into a reqwest header

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    secret_env::dotenv().ok();
    let mut api_key = secret_env::require("OPENAI_API_KEY")?;

    let client = reqwest::Client::new();
    let request = api_key.with_str(|key| {
        client
            .get("https://api.openai.com/v1/models")
            .bearer_auth(key)
    })?;

    let models = request.send().await?.error_for_status()?.text().await?;
    println!("{models}");
    Ok(())
}
```

`bearer_auth` marks the `Authorization` header as sensitive, so its `Debug`
output shows `Sensitive` instead of the token.

### `DATABASE_URL` into `PgPool::connect`

`PgPool::connect(&str)` returns a future that borrows the URL, so it cannot
leave the closure. Parse the options inside the closure and connect after it:

```rust
use sqlx::postgres::{PgConnectOptions, PgPool};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    secret_env::dotenv().ok();
    let mut database_url = secret_env::require("DATABASE_URL")?;

    let options: PgConnectOptions = database_url.with_str(str::parse)??;
    let pool = PgPool::connect_with(options).await?;

    let one: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&pool).await?;
    println!("{one}");
    Ok(())
}
```

The first `?` handles a memory error and the second handles a URL parse error.

### Optional `SENTRY_DSN` and a 4096-byte key

An RSA-4096 PEM is about 3.2 KB, which is more than the 2048-byte default:

```rust
use jsonwebtoken::EncodingKey;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    secret_env::dotenv().ok();

    let _sentry = match secret_env::var("SENTRY_DSN")? {
        Some(mut dsn) => Some(dsn.with_str(|dsn| sentry::init(dsn))?),
        None => None,
    };

    let mut pem = secret_env::require_sized::<4096>("JWT_PRIVATE_KEY")?;
    let signing_key = pem.with(EncodingKey::from_rsa_pem)??;
    // jsonwebtoken::encode(&header, &claims, &signing_key)?;
    Ok(())
}
```

Keep the `sentry::init` guard alive for the life of the program. Write
`|dsn| sentry::init(dsn)` instead of passing `sentry::init` directly: the
function item is tied to a single lifetime, so it does not compile.

## `dotenv()`

`dotenv()` calls `dotenvy::dotenv()` and behaves the same way:

- It loads `.env` from the current directory or the nearest parent directory.
- It does not override variables that are already set.
- A missing file is an error. Call `.ok()` if the file is optional.
- It writes to the process environment, so call it at the start of `main`,
  before you start any threads or a multi-threaded runtime.

For parse errors, `Error::Dotenv` reports the position but not the line text.
dotenvy's own error includes the line, which could contain a secret.

## What is protected

- The `String` from `std::env::var` is copied into the `Secret` page, then
  volatile-zeroed, including its spare capacity. This also happens when the
  value is too long, and for non-Unicode values.
- After that, the value exists only in the `Secret`. That page is locked
  against swap and excluded from core dumps on Linux. Outside `with` calls it
  is inaccessible on Unix and read-only on Windows.

## What is not protected

- **The process environment.** This crate never unsets variables, so the
  original value stays in the environment block. Child processes inherit it,
  and anyone with the right access can read it, for example through
  `/proc/<pid>/environ`.
- **Copies made by your closure.** Headers, connection options, DSNs and
  parsed keys are ordinary heap memory owned by those libraries.
- **dotenvy's buffers.** Data that dotenvy reads from `.env` is not zeroed.
- **Timing.** Comparing secret bytes with `==` is not constant-time.

## Out of scope

There is no support for CLI argument parsing (clap), web framework
extractors (axum), reading from stdin, unsetting the environment, or cloud
secret managers.

## MSRV

Rust 1.85, edition 2024.

## License

MIT
