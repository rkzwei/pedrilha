use anyhow::{Context, Result};
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

/// Send a magic-link email via SMTP.
///
/// Config is read from env vars at call time so the service is stateless
/// and picks up changes without a restart.
///
/// Required env vars: SMTP_HOST, SMTP_PORT, SMTP_USER, SMTP_PASSWORD, SMTP_FROM
/// Optional: APP_URL (defaults to http://localhost:3000)
pub async fn send_magic_link(to_email: &str, token: &str, next: Option<&str>) -> Result<()> {
    let host = std::env::var("SMTP_HOST").context("SMTP_HOST not set")?;
    let port: u16 = std::env::var("SMTP_PORT")
        .unwrap_or_else(|_| "587".into())
        .parse()
        .context("SMTP_PORT must be a number")?;
    let user = std::env::var("SMTP_USER").context("SMTP_USER not set")?;
    let password = std::env::var("SMTP_PASSWORD").context("SMTP_PASSWORD not set")?;
    let from = std::env::var("SMTP_FROM").unwrap_or_else(|_| user.clone());
    let app_url = std::env::var("APP_URL").unwrap_or_else(|_| {
        tracing::warn!("APP_URL not set — magic-link emails will contain localhost URLs. Set APP_URL=https://yourdomain.com in production.");
        "http://localhost:8080".into()
    });

    let magic_url = match next.filter(|n| !n.is_empty()) {
        Some(n) => format!(
            "{}/auth/verify?token={}&next={}",
            app_url.trim_end_matches('/'),
            token,
            n
        ),
        None => format!("{}/auth/verify?token={}", app_url.trim_end_matches('/'), token),
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
