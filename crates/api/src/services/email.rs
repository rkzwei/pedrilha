use anyhow::{Context, Result};
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

/// Validate a request's `Origin` header before trusting it to build a magic-link URL.
///
/// The `Origin` header is attacker-controllable on any non-browser HTTP client (curl,
/// scripts, etc.), so it must be checked against an allowlist before use — otherwise
/// anyone could make our SMTP relay send an email containing a link to an arbitrary
/// domain of their choosing.
///
/// The allowlist is `CORS_ORIGINS` (same env var `main.rs` uses to build the `CorsLayer`),
/// so there is a single source of truth for "domains this deployment serves."
///
/// If `CORS_ORIGINS` is unset, CORS itself is running in permissive/dev mode (see
/// `main.rs`), so there is no boundary to enforce — any well-formed http(s) origin is
/// accepted as a dev convenience.
fn validate_request_origin(origin: &str) -> Option<String> {
    let origin = origin.trim().trim_end_matches('/');
    if origin.is_empty() {
        return None;
    }

    let configured = std::env::var("CORS_ORIGINS").unwrap_or_default();
    let allowed: Vec<&str> = configured
        .split(',')
        .map(|s| s.trim().trim_end_matches('/'))
        .filter(|s| !s.is_empty())
        .collect();

    if allowed.is_empty() {
        return (origin.starts_with("http://") || origin.starts_with("https://"))
            .then(|| origin.to_string());
    }

    allowed.contains(&origin).then(|| origin.to_string())
}

/// Send a magic-link email via SMTP.
///
/// Config is read from env vars at call time so the service is stateless
/// and picks up changes without a restart.
///
/// Required env vars: SMTP_HOST, SMTP_PORT, SMTP_USER, SMTP_PASSWORD, SMTP_FROM
///
/// The link's base URL is resolved in this order:
/// 1. `request_origin` — the caller's `Origin` header, if it validates against
///    `CORS_ORIGINS` (see [`validate_request_origin`]). This is what makes the link
///    reflect whichever domain the sign-in request actually came from, instead of
///    always pointing at one hardcoded deployment.
/// 2. `APP_URL` env var — static fallback for single-domain deployments or when the
///    request had no usable Origin header (e.g. a same-origin fetch some browsers omit
///    Origin on, or a non-browser caller).
/// 3. `http://localhost:8080` — last-resort dev fallback.
pub async fn send_magic_link(
    to_email: &str,
    token: &str,
    next: Option<&str>,
    request_origin: Option<&str>,
) -> Result<()> {
    let host = std::env::var("SMTP_HOST").context("SMTP_HOST not set")?;
    let port: u16 = std::env::var("SMTP_PORT")
        .unwrap_or_else(|_| "587".into())
        .parse()
        .context("SMTP_PORT must be a number")?;
    let user = std::env::var("SMTP_USER").context("SMTP_USER not set")?;
    let password = std::env::var("SMTP_PASSWORD").context("SMTP_PASSWORD not set")?;
    let from = std::env::var("SMTP_FROM").unwrap_or_else(|_| user.clone());
    let app_url = request_origin
        .and_then(validate_request_origin)
        .or_else(|| std::env::var("APP_URL").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| {
            tracing::warn!("No valid request Origin and APP_URL not set — magic-link emails will contain localhost URLs. Set APP_URL=https://yourdomain.com and/or CORS_ORIGINS in production.");
            "http://localhost:8080".into()
        });

    let magic_url = match next.filter(|n| !n.is_empty()) {
        Some(n) => format!(
            "{}/auth/verify?token={}&next={}",
            app_url.trim_end_matches('/'),
            token,
            n
        ),
        None => format!(
            "{}/auth/verify?token={}",
            app_url.trim_end_matches('/'),
            token
        ),
    };

    let body = format!(
        "Click the link below to sign in to Gem Finder.\n\
        The link expires in 15 minutes.\n\n\
        {}\n\n\
        If you didn't request this, you can ignore this email.\n",
        magic_url
    );

    let email = Message::builder()
        .from(from.parse().context("invalid SMTP_FROM address")?)
        .to(to_email.parse().context("invalid recipient address")?)
        .subject("Your Gem Finder sign-in link")
        .header(ContentType::TEXT_PLAIN)
        .body(body)
        .context("failed to build email message")?;

    let creds = Credentials::new(user, password);

    // Port 465 = implicit TLS (SMTPS); everything else uses STARTTLS
    if port == 465 {
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
            .context("failed to create SMTPS transport")?
            .port(port)
            .credentials(creds)
            .build();
        mailer.send(email).await.context("SMTP send failed")?;
    } else {
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)
            .context("failed to create STARTTLS transport")?
            .port(port)
            .credentials(creds)
            .build();
        mailer.send(email).await.context("SMTP send failed")?;
    }

    Ok(())
}
