use crate::{component, service, Context};
use embassy_executor::Spawner;

/// Runs once at startup: wait for Wi-Fi, then say what we are about to do.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    ctx.logger.info("Waiting for Wi-Fi connection...");
    component!(ctx, home).wait_connected().await;
    ctx.logger.info("Connected. Syncing status over HTTPS.");
}

/// Per-tick loop: one POST, then sleep.
pub async fn forever(ctx: &'static Context) {
    let https = service!(ctx, secure_client);
    let mut rx_buf = [0u8; 1024];
    let payload = b"{\"device\":\"esp32c3\",\"status\":\"online\"}";

    match https
        .post(
            "https://example.com/api/status",
            "application/json",
            payload,
            &mut rx_buf,
        )
        .await
    {
        Ok(response) => ctx
            .logger
            .info(format_args!("POST ok, {} bytes", response.len())),
        Err(err) => ctx.logger.error(format_args!("POST failed: {err:?}")),
    }

    ctx.delay.delay_ms(30000).await;
}
