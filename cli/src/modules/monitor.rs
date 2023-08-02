use crate::imports::*;
use workflow_core::channel::*;
use workflow_terminal::clear::*;
use workflow_terminal::cursor::*;

pub struct Monitor {
    shutdown_tx: Arc<Mutex<Option<Sender<()>>>>,
}

impl Default for Monitor {
    fn default() -> Self {
        Monitor { shutdown_tx: Arc::new(Mutex::new(None)) }
    }
}

#[async_trait]
impl Handler for Monitor {
    fn verb(&self, _ctx: &Arc<dyn Context>) -> Option<&'static str> {
        Some("monitor")
    }

    fn help(&self, _ctx: &Arc<dyn Context>) -> &'static str {
        "Balance monitor"
    }

    async fn stop(self: Arc<Self>, _ctx: &Arc<dyn Context>) -> cli::Result<()> {
        let shutdown_tx = self.shutdown_tx.lock().unwrap().take();
        if let Some(shutdown_tx) = shutdown_tx {
            shutdown_tx.send(()).await.ok();
        }
        Ok(())
    }

    async fn handle(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, cmd: &str) -> cli::Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        self.main(&ctx, argv, cmd).await.map_err(|e| e.into())
    }
}

impl Monitor {
    async fn main(self: Arc<Self>, ctx: &Arc<KaspaCli>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let (shutdown_tx, shutdown_rx) = oneshot();
        self.shutdown_tx.lock().unwrap().replace(shutdown_tx.clone());
        let mut interval = interval(Duration::from_millis(1000));

        let term = ctx.term();
        spawn(async move {
            term.kbhit(None).await.ok();
            shutdown_tx.send(()).await.ok();
        });

        let ctx = ctx.clone();
        let this = self.clone();
        spawn(async move {
            loop {
                select! {

                    _ = interval.next().fuse() => {
                        this.redraw(&ctx).await.ok();
                        yield_executor().await;
                    }

                    _ = shutdown_rx.recv().fuse() => {
                        break;
                    }

                }
            }

            tprint!(ctx, "{ClearScreen}");
            this.shutdown_tx.lock().unwrap().take();
            ctx.term().refresh_prompt();
        });

        Ok(())
    }

    async fn redraw(self: &Arc<Self>, ctx: &Arc<KaspaCli>) -> Result<()> {
        tprint!(ctx, "{}", ClearScreen);
        tprint!(ctx, "{}", Goto(1, 1));

        log_info!("moinitor redrawing...");

        if !ctx.wallet().is_connected() {
            tprintln!(ctx, "{}", style("Wallet is not connected to the network").magenta());
            tprintln!(ctx);
        } else if !ctx.wallet().is_synced() {
            tprintln!(ctx, "{}", style("Kaspa node is currently syncing").magenta());
            tprintln!(ctx);
        }

        ctx.list().await?;

        Ok(())
    }
}
