use crate::error::Error;
use crate::imports::*;
use crate::modules::*;
use crate::result::Result;
use crate::utils::*;
use async_trait::async_trait;
use cfg_if::cfg_if;
use futures::stream::{Stream, StreamExt, TryStreamExt};
use futures::*;
use kaspa_wallet_core::imports::{AtomicBool, Ordering, ToHex};
use kaspa_wallet_core::runtime::wallet::WalletCreateArgs;
use kaspa_wallet_core::storage::interface::AccessContext;
use kaspa_wallet_core::storage::{AccessContextT, AccountKind, IdT, PrvKeyDataId, PrvKeyDataInfo};
use kaspa_wallet_core::utxo;
use kaspa_wallet_core::{runtime::wallet::AccountCreateArgs, runtime::Wallet, secret::Secret, Events};
use pad::PadStr;
use separator::Separatable;
use std::ops::Deref;
use std::sync::{Arc, Mutex};
use workflow_core::channel::*;
use workflow_core::time::Instant;
use workflow_log::*;
use workflow_terminal::*;
pub use workflow_terminal::{parse, Cli, Options as TerminalOptions, Result as TerminalResult, TargetElement as TerminalTarget}; //{CrLf, Terminal};

pub struct WalletCli {
    term: Arc<Mutex<Option<Arc<Terminal>>>>,
    wallet: Arc<Wallet>,
    notifications_task_ctl: DuplexChannel,
    mute: Arc<AtomicBool>,
    flags: Flags,
    last_interaction: Arc<Mutex<Instant>>,
    handlers: Arc<HandlerCli>,
}

impl From<&WalletCli> for Arc<Terminal> {
    fn from(ctx: &WalletCli) -> Arc<Terminal> {
        ctx.term()
    }
}

impl AsRef<WalletCli> for WalletCli {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl workflow_log::Sink for WalletCli {
    fn write(&self, _target: Option<&str>, _level: Level, args: &std::fmt::Arguments<'_>) -> bool {
        if let Some(term) = self.try_term() {
            term.writeln(args.to_string());
            true
        } else {
            false
        }
    }
}

impl WalletCli {
    fn new(wallet: Arc<Wallet>) -> Self {
        WalletCli {
            term: Arc::new(Mutex::new(None)),
            wallet,
            notifications_task_ctl: DuplexChannel::oneshot(),
            mute: Arc::new(AtomicBool::new(false)),
            flags: Flags::default(),
            last_interaction: Arc::new(Mutex::new(Instant::now())),
            handlers: Arc::new(HandlerCli::default()),
            // context : Arc::new(Mutex::new(None)),
        }
    }

    pub fn term(&self) -> Arc<Terminal> {
        self.term.lock().unwrap().as_ref().cloned().expect("WalletCli::term is not initialized")
    }

    pub fn try_term(&self) -> Option<Arc<Terminal>> {
        self.term.lock().unwrap().as_ref().cloned()
    }

    pub fn wallet(&self) -> Arc<Wallet> {
        self.wallet.clone()
    }

    pub fn store(&self) -> Arc<dyn Interface> {
        self.wallet.store().clone()
    }

    pub fn handler(&self) -> Arc<HandlerCli> {
        self.handlers.clone()
    }

    pub fn flags(&self) -> &Flags {
        &self.flags
    }

    pub fn toggle_mute(&self) -> &'static str {
        utils::toggle(&self.mute)
    }

    // fn ctx(&self) -> Arc<context::Wallet> {
    //     self.context.lock().unwrap().as_ref().unwrap().clone()
    // }

    pub fn is_mutted(&self) -> bool {
        self.mute.load(Ordering::SeqCst)
    }

    /*

        async fn action(&self, action: Action, mut argv: Vec<String>, term: Arc<Terminal>, cmd: &str) -> Result<()> {
            argv.remove(0);

            match action {
                Action::Help => {
                    term.writeln("\n\rCommands:\n\r");
                    display_help(&term);
                }
                Action::Halt => {
                    panic!("halting on user request...");
                }
                Action::Exit => {
                    term.writeln("bye!");
                    #[cfg(not(target_arch = "wasm32"))]
                    term.exit().await;
                    #[cfg(target_arch = "wasm32")]
                    workflow_dom::utils::window().location().reload().ok();
                }
                Action::Set => {
                    if argv.is_empty() {
                        println!("\nSettings:\n");
                        let list = Settings::list();
                        let list = list
                            .iter()
                            .map(|setting| {
                                let value: String = self.wallet.settings().get(setting.clone()).unwrap_or_else(|| "-".to_string());
                                let descr = setting.descr();
                                (setting.to_lowercase_string(), value, descr)
                            })
                            .collect::<Vec<(_, _, _)>>();
                        let c1 = list.iter().map(|(c, _, _)| c.len()).fold(0, |a, b| a.max(b)) + 4;
                        let c2 = list.iter().map(|(_, c, _)| c.len()).fold(0, |a, b| a.max(b)) + 4;

                        list.iter().for_each(|(k, v, d)| {
                            println!("{}: {} \t {}", k.pad_to_width_with_alignment(c1, pad::Alignment::Right), v.pad_to_width(c2), d);
                        });
                    } else if argv.len() != 2 {
                        println!("\n\rError:\n\r");
                        println!("Usage:\n\rset <key> <value>");
                        return Ok(());
                    } else {
                        let key = argv[0].as_str();
                        let value = argv[1].as_str().trim();

                        if value.contains(' ') || value.contains('\t') {
                            return Err(Error::Custom("Whitespace in settings is not allowed".to_string()));
                        }

                        match key {
                            "network" => {
                                let network: NetworkType = value.parse().map_err(|_| "Unknown network type".to_string())?;
                                self.wallet.settings().set(Settings::Network, network).await?;
                            }
                            "server" => {
                                self.wallet.settings().set(Settings::Server, value).await?;
                            }
                            "wallet" => {
                                self.wallet.settings().set(Settings::Wallet, value).await?;
                            }
                            _ => return Err(Error::Custom(format!("Unknown setting '{}'", key))),
                        }
                        self.wallet.settings().try_store().await?;
                    }
                }
                Action::Mute => {
                    println!("mute is {}", toggle(&self.mute));
                }
                Action::Track => {
                    if let Some(attr) = argv.first() {
                        let track: Track = attr.parse()?;
                        self.flags.toggle(track);
                    } else {
                        for flag in self.flags.map().iter() {
                            let k = flag.key().to_string();
                            let v = flag.value().load(Ordering::SeqCst);
                            let s = if v { "on" } else { "off" };
                            println!("{k} is {s}");
                        }
                    }
                }
                Action::Hint => {
                    if !argv.is_empty() {
                        let hint = cmd.replace("hint", "");
                        let hint = hint.trim();
                        let store = self.wallet.store();
                        if hint == "remove" {
                            store.set_user_hint(None).await?;
                        } else {
                            store.set_user_hint(Some(hint.into())).await?;
                        }
                    } else {
                        println!("Usage:\n'hint <text>' or 'hint remove'");
                    }
                }
                Action::Connect => {
                    let url = argv.first().cloned().or_else(|| self.wallet.settings().get(Settings::Server));

                    let network_type = self.wallet.network()?;
                    let url = self.wallet.rpc_client().parse_url(url, network_type)?;
                    println!("Connecting to {}...", url.clone().unwrap_or_else(|| "default".to_string()));

                    let options = ConnectOptions { block_async_connect: true, strategy: ConnectStrategy::Fallback, url };
                    self.wallet.rpc_client().connect(options).await?;
                }
                Action::Disconnect => {
                    self.wallet.rpc_client().shutdown().await?;
                }
                Action::GetInfo => {
                    let response = self.wallet.get_info().await?;
                    println!("{response}");
                }
                Action::Metrics => {
                    let response = self.wallet.rpc().get_metrics(true, true).await.map_err(|e| e.to_string())?;
                    println!("{:#?}", response);
                }
                Action::Ping => {
                    if self.wallet.ping().await {
                        println!("ping ok");
                    } else {
                        error!("ping error");
                    }
                }
                // Action::Balance => {}
                //     let accounts = self.wallet.accounts();
                //     for account in accounts {
                //         let balance = account.balance();
                //         let name = account.name();
                //         log_info!("{name} - {balance} KAS");
                //     }
                // }
                Action::Create => {
                    let is_open = self.wallet.is_open()?;

                    let op = if argv.is_empty() { if is_open { "account" } else { "wallet" }.to_string() } else { argv.remove(0) };

                    match op.as_str() {
                        "wallet" => {
                            let wallet_name = if argv.is_empty() {
                                None
                            } else {
                                let name = argv.remove(0);
                                let name = name.trim().to_string();

                                Some(name)
                            };

                            let wallet_name = wallet_name.as_deref();
                            self.create_wallet(wallet_name).await?;
                        }
                        "account" => {
                            if !is_open {
                                return Err(Error::WalletIsNotOpen);
                            }

                            let account_kind = if argv.is_empty() {
                                AccountKind::Bip32
                            } else {
                                let kind = argv.remove(0);
                                kind.parse::<AccountKind>()?
                            };

                            let account_name = if argv.is_empty() {
                                None
                            } else {
                                let name = argv.remove(0);
                                let name = name.trim().to_string();

                                Some(name)
                            };

                            // TODO - switch to selection; temporarily use existing account
                            let account = self.wallet.account()?;
                            let prv_key_data_id = account.prv_key_data_id;

                            let account_name = account_name.as_deref();
                            self.create_account(prv_key_data_id, account_kind, account_name).await?;
                        }
                        _ => {
                            println!("\nError:\n");
                            println!("Usage:\ncreate <account|wallet>");
                            return Ok(());
                        }
                    }
                }
                Action::Network => {
                    if let Some(network_type) = argv.first() {
                        let network_type: NetworkType =
                            network_type.trim().parse::<NetworkType>().map_err(|_| "Unknown network type: `{network_type}`")?;
                        // .map_err(|err|err.to_string())?;
                        println!("Setting network type to: {network_type}");
                        self.wallet.select_network(network_type)?;
                        self.wallet.settings().set(Settings::Network, network_type).await?;
                        // self.wallet.settings().try_store().await?;
                    } else {
                        let network_type = self.wallet.network()?;
                        println!("Current network type is: {network_type}");
                    }
                }
                Action::Server => {
                    if let Some(url) = argv.first() {
                        self.wallet.settings().set(Settings::Server, url).await?;
                        println!("Setting RPC server to: {url}");
                    } else {
                        let server = self.wallet.settings().get(Settings::Server).unwrap_or_else(|| "n/a".to_string());
                        println!("Current RPC server is: {server}");
                    }
                }
                Action::Broadcast => {
                    self.wallet.broadcast().await?;
                }
                Action::CreateUnsignedTx => {
                    let account = self.wallet.account()?;
                    account.create_unsigned_transaction().await?;
                }
                Action::DumpUnencrypted => {
                    let account = self.wallet.account()?;
                    let password = Secret::new(term.ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
                    let mut _payment_secret = Option::<Secret>::None;

                    if self.wallet.is_account_key_encrypted(&account).await?.is_some_and(|flag| flag) {
                        _payment_secret = Some(Secret::new(term.ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec()));
                    }
                    let keydata = self.wallet.get_prv_key_data(password.clone(), &account.prv_key_data_id).await?;
                    if keydata.is_none() {
                        return Err("It is read only wallet.".into());
                    }

                    todo!();

                    // let (mnemonic, xprv) = self.wallet.dump_unencrypted(account, password, payment_secret).await?;
                    // term.writeln(format!("mnemonic: {mnemonic}"));
                    // term.writeln(format!("xprv: {xprv}"));
                }
                Action::NewAddress => {
                    let account = self.wallet.account()?;
                    let response = account.new_receive_address().await?;
                    println!("{response}");
                }
                // Action::Parse => {
                //     self.wallet.parse().await?;
                // }
                Action::Estimate => {}
                Action::Send => {
                    // address, amount, priority fee
                    let account = self.wallet.account()?;

                    if argv.len() < 2 {
                        return Err("Usage: send <address> <amount> <priority fee>".into());
                    }

                    let address = argv.get(0).unwrap();
                    let amount = argv.get(1).unwrap();
                    let priority_fee = argv.get(2);

                    let priority_fee_sompi = if let Some(fee) = priority_fee { Some(helpers::kas_str_to_sompi(fee)?) } else { None };

                    let address = Address::try_from(address.as_str())?;
                    let amount_sompi = helpers::kas_str_to_sompi(amount)?;

                    let wallet_secret = Secret::new(term.ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
                    // let mut payment_secret = Option::<Secret>::None;

                    let payment_secret = if self.wallet.is_account_key_encrypted(&account).await?.is_some_and(|f| f) {
                        Some(Secret::new(term.ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec()))
                    } else {
                        None
                    };
                    // let keydata = self.wallet.get_prv_key_data(wallet_secret.clone(), &account.prv_key_data_id).await?;
                    // if keydata.is_none() {
                    //     return Err("It is read only wallet.".into());
                    // }
                    let abortable = Abortable::default();

                    let outputs = PaymentOutputs::try_from((address.clone(), amount_sompi))?;
                    let ids =
                        // account.send(&address, amount_sompi, priority_fee_sompi, keydata.unwrap(), payment_secret, &abortable).await?;
                        account.send(&outputs, priority_fee_sompi, false, wallet_secret, payment_secret, &abortable).await?;

                    println!("\nSending {amount} KAS to {address}, tx ids:");
                    println!("{}\n", ids.into_iter().map(|a| a.to_string()).collect::<Vec<_>>().join("\n"));
                }
                Action::Address => {
                    let address = self.account().await?.receive_address().await?.to_string();
                    println!("\n{address}\n");
                }
                Action::ShowAddresses => {
                    let manager = self.wallet.account()?.receive_address_manager()?;
                    let index = manager.index()?;
                    let addresses = manager.get_range_with_args(0..index, false).await?;
                    println!("Receive addresses: 0..{index}");
                    println!("{}\n", addresses.into_iter().map(|a| a.to_string()).collect::<Vec<_>>().join("\n"));

                    let manager = self.wallet.account()?.change_address_manager()?;
                    let index = manager.index()?;
                    let addresses = manager.get_range_with_args(0..index, false).await?;
                    println!("Change addresses: 0..{index}");
                    println!("{}\n", addresses.into_iter().map(|a| a.to_string()).collect::<Vec<_>>().join("\n"));
                }
                Action::Sign => {
                    self.wallet.account()?.sign().await?;
                }
                Action::Sweep => {
                    self.wallet.account()?.sweep().await?;
                }
                Action::SubscribeDaaScore => {
                    self.wallet.subscribe_daa_score().await?;
                }
                Action::UnsubscribeDaaScore => {
                    self.wallet.unsubscribe_daa_score().await?;
                }

                // ~~~
                Action::Export => {
                    if argv.is_empty() || argv.get(0) == Some(&"help".to_string()) {
                        println!("Usage: export [mnemonic]");
                        return Ok(());
                    }

                    let what = argv.get(0).unwrap();
                    match what.as_str() {
                        "mnemonic" => {
                            let account = self.account().await?;
                            let prv_key_data_id = account.prv_key_data_id;

                            let wallet_secret = Secret::new(term.ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
                            if wallet_secret.as_ref().is_empty() {
                                return Err(Error::WalletSecretRequired);
                            }

                            let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
                            let prv_key_data = self.wallet.store().as_prv_key_data_store()?.load_key_data(&ctx, &prv_key_data_id).await?;
                            if let Some(keydata) = prv_key_data {
                                let payment_secret = if keydata.payload.is_encrypted() {
                                    let payment_secret =
                                        Secret::new(term.ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec());
                                    if payment_secret.as_ref().is_empty() {
                                        return Err(Error::PaymentSecretRequired);
                                    } else {
                                        Some(payment_secret)
                                    }
                                } else {
                                    None
                                };

                                let prv_key_data = keydata.payload.decrypt(payment_secret.as_ref())?;
                                let mnemonic = prv_key_data.as_ref().as_mnemonic()?;
                                if let Some(mnemonic) = mnemonic {
                                    if payment_secret.is_none() {
                                        println!("mnemonic:");
                                        println!("");
                                        println!("{}", mnemonic.phrase());
                                        println!("");
                                    } else {
                                        para!(
                                            "\
                                            IMPORTANT: to recover your private key using this mnemonic in the future \
                                            you will need your payment password. Your payment password is permanently associated with \
                                            this mnemonic.",
                                        );
                                        println!("");
                                        println!("mnemonic:");
                                        println!("");
                                        println!("{}", mnemonic.phrase());
                                        println!("");
                                    }
                                } else {
                                    println!("mnemonic is not available for this private key");
                                }
                            } else {
                                return Err(Error::KeyDataNotFound);
                            }

                            // account
                            // log_info!("selected account: {}", account.name_or_id());
                        }
                        _ => {
                            return Err(format!("Invalid argument: {}", what).into());
                        }
                    }
                }
                Action::Import => {
                    if argv.is_empty() || argv.get(0) == Some(&"help".to_string()) {
                        println!("Usage: import [mnemonic]");
                        return Ok(());
                    }

                    let what = argv.get(0).unwrap();
                    match what.as_str() {
                        "mnemonic" => {
                            let mnemonic = helpers::ask_mnemonic(&term).await?;
                            println!("Mnemonic: {:?}", mnemonic);
                        }
                        "legacy" => {
                            if exists_v0_keydata().await? {
                                let import_secret = Secret::new(
                                    term.ask(true, "Enter the password for the wallet you are importing:")
                                        .await?
                                        .trim()
                                        .as_bytes()
                                        .to_vec(),
                                );
                                let wallet_secret =
                                    Secret::new(term.ask(true, "Enter wallet password:").await?.trim().as_bytes().to_vec());
                                self.wallet.import_gen0_keydata(import_secret, wallet_secret).await?;
                            } else if application_runtime::is_web() {
                                return Err("'kaspanet' web wallet storage not found at this domain name".into());
                            } else {
                                return Err("KDX/kaspanet keydata file not found".into());
                            }
                        }
                        "kaspa-wallet" => {}
                        _ => {
                            return Err(format!("Invalid argument: {}", what).into());
                        }
                    }
                }

                // ~~~
                Action::List => {
                    println!();
                    let mut keys = self.wallet.keys().await?;
                    while let Some(key) = keys.try_next().await? {
                        println!("• pk{key}");
                        let mut accounts = self.wallet.accounts(Some(key.id)).await?;
                        while let Some(account) = accounts.try_next().await? {
                            println!("    {}", account.get_list_string()?);
                            // term.writeln(format!("    {}", account.get_ls_string()?));
                        }
                        println!();
                    }
                }
                Action::Name => {
                    if argv.is_empty() {
                        let account = self.account().await?;
                        let id = account.id().to_hex();
                        let name = account.name();
                        let name = if name.is_empty() { "no name".to_string() } else { name };

                        println!("\nname: {name}  account id: {id}\n");
                    } else {
                        // let account = self.account().await?;
                    }
                }
                Action::Select => {
                    if argv.is_empty() {
                        let account = self.prompt_account().await?;
                        self.wallet.select(Some(&account)).await?;
                    } else {
                        let pat = argv.remove(0);
                        let pat = pat.as_str();
                        let accounts = self.wallet.active_accounts().inner().values().cloned().collect::<Vec<_>>();
                        let matches = accounts
                            .into_iter()
                            .filter(|account| account.name().starts_with(pat) || account.id().to_hex().starts_with(pat))
                            .collect::<Vec<_>>();
                        if matches.is_empty() {
                            return Err(Error::AccountNotFound(pat.to_string()));
                        } else if matches.len() > 1 {
                            return Err(Error::AmbigiousAccount(pat.to_string()));
                        } else {
                            let account = matches[0].clone();
                            self.wallet.select(Some(&account)).await?;
                        };
                    }
                }
                Action::Open => {
                    let name = if let Some(name) = argv.first().cloned() {
                        Some(name)
                    } else {
                        self.wallet.settings().get(Settings::Wallet).clone()
                    };

                    let secret = Secret::new(term.ask(true, "Enter wallet password:").await?.trim().as_bytes().to_vec());
                    self.wallet.load(secret, name).await?;
                }
                Action::Close => {
                    self.wallet.reset().await?;
                }

                #[cfg(target_arch = "wasm32")]
                Action::Reload => {
                    workflow_dom::utils::window().location().reload().ok();
                }
            }

            Ok(())
        }
    */

    async fn start(self: &Arc<Self>) -> Result<()> {
        self.notification_pipe_task();
        Ok(())
    }

    async fn stop(self: &Arc<Self>) -> Result<()> {
        self.notifications_task_ctl.signal(()).await?;
        Ok(())
    }

    pub fn notification_pipe_task(self: &Arc<Self>) {
        let this = self.clone();
        // let _term = self.term().unwrap_or_else(|| panic!("WalletCli::notification_pipe_task(): `term` is not initialized"));

        // let notification_channel_receiver = self.wallet.rpc_client().notification_channel_receiver();
        let multiplexer = MultiplexerChannel::from(self.wallet.multiplexer());
        workflow_core::task::spawn(async move {
            // term.writeln(args.to_string());
            loop {
                select! {

                    _ = this.notifications_task_ctl.request.receiver.recv().fuse() => {
                        // if let Ok(msg) = msg {
                        //     let text = format!("{msg:#?}").replace('\n',"\r\n");
                        //     println!("#### text: {text:?}");
                        //     term.pipe_crlf.send(text).await.unwrap_or_else(|err|log_error!("WalletCli::notification_pipe_task() unable to route to term: `{err}`"));
                        // }
                        break;
                    },
                    // msg = notification_channel_receiver.recv().fuse() => {
                    //     if let Ok(msg) = msg {

                    //         log_info!("Received RPC notification: {msg:#?}");
                    //         let text = format!("{msg:#?}").crlf();//replace('\n',"\r\n"); //.payload);
                    //         term.pipe_crlf.send(text).await.unwrap_or_else(|err|log_error!("WalletCli::notification_pipe_task() unable to route to term: `{err}`"));
                    //     }
                    // },

                    msg = multiplexer.receiver.recv().fuse() => {

                        if let Ok(msg) = msg {
                            match msg {
                                Events::Connect(_url) => {
                                    // log_info!("Connected to {url}");
                                },
                                Events::Disconnect(url) => {
                                    println!("Disconnected from {url}");
                                },
                                Events::UtxoIndexNotEnabled => {
                                    println!("Error: Kaspa node UTXO index is not enabled...")
                                },
                                Events::ServerStatus {
                                    is_synced,
                                    server_version,
                                    url,
                                    // has_utxo_index,
                                    ..
                                } => {

                                    tprintln!(this, "Connected to Kaspa node version {server_version} at {url}\n");


                                    // log_info!("Server version server {server_version}");
                                    let is_open = this.wallet.is_open().unwrap_or_else(|err| { terrorln!(this, "Unable to check if wallet is open: {err}"); false });

                                    if !is_synced {
                                        if is_open {
                                            terrorln!(this, "Error: Unable to sync wallet - Kaspa node is not synced...");

                                        } else {
                                            terrorln!(this, "Error: Kaspa node is not synced...");
                                        }
                                    }
                                },
                                Events::WalletHasLoaded {
                                    hint
                                } => {

                                    if let Some(hint) = hint {
                                        tprintln!(this, "\nYour wallet hint is: {hint}\n");
                                    }

                                    this.list().await.unwrap_or_else(|err|terrorln!(this, "{err}"));
                                },
                                Events::UtxoProcessor(event) => {

                                    match event {

                                        utxo::Events::DAAScoreChange(daa) => {
                                            if this.is_mutted() && this.flags.get(Track::Daa) {
                                                tprintln!(this, "DAAScoreChange: {daa}");
                                            }
                                        },
                                        utxo::Events::Pending {
                                            record
                                        } => {
                                            if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Utxo)) {
                                                let tx = record.format(&this.wallet);
                                                tprintln!(this, "pending {tx}");
                                            }
                                        },
                                        utxo::Events::Reorg {
                                            record
                                        } => {
                                            if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Utxo)) {
                                                let tx = record.format(&this.wallet);
                                                tprintln!(this, "pending {tx}");
                                            }
                                        },
                                        utxo::Events::External {
                                            record
                                        } => {
                                            if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Utxo)) {
                                                let tx = record.format(&this.wallet);
                                                tprintln!(this,"external {tx}");
                                            }
                                        },
                                        utxo::Events::Maturity {
                                            record
                                        } => {
                                            if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Utxo)) {
                                                let tx = record.format(&this.wallet);
                                                tprintln!(this,"maturity {tx}");
                                            }
                                        },
                                        utxo::Events::Debit {
                                            record
                                        } => {
                                            if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Utxo)) {
                                                let tx = record.format(&this.wallet);
                                                tprintln!(this,"{tx}");
                                            }
                                        },
                                        utxo::Events::Balance {
                                            balance,
                                            id,
                                            mature_utxo_size,
                                            pending_utxo_size,
                                        } => {

                                            if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Balance)) {
                                                let network_type = this.wallet.network().expect("missing network type");
                                                let balance = BalanceStrings::from((&balance,&network_type, Some(19)));
                                                let id = id.short();

                                                let pending_utxo_info = if pending_utxo_size > 0 {
                                                    format!("({pending_utxo_size} pending)")
                                                } else { "".to_string() };
                                                let utxo_info = style(format!("{} UTXOs {pending_utxo_info}", mature_utxo_size.separated_string())).dim();

                                                tprintln!(this, "{} {id}: {balance}   {utxo_info}",style("balance".pad_to_width(8)).blue());
                                            }
                                        },
                                    }
                                }
                            }
                        }

                    }
                }
            }

            this.notifications_task_ctl
                .response
                .sender
                .send(())
                .await
                .unwrap_or_else(|err| log_error!("WalletCli::notification_pipe_task() unable to signal task shutdown: `{err}`"));
        });
    }

    // ---

    pub(crate) async fn create_wallet(&self, name: Option<&str>) -> Result<()> {
        // use kaspa_wallet_core::error::Error;

        let term = self.term();

        if self.wallet.exists(name).await? {
            tprintln!(self, "WARNING - A previously created wallet already exists!");

            let overwrite = term
                .ask(false, "Are you sure you want to overwrite it (type 'y' to approve)?: ")
                .await?
                .trim()
                .to_string()
                .to_lowercase();
            if overwrite.ne("y") {
                return Ok(());
            }
        }

        let account_title = term.ask(false, "Default account title: ").await?.trim().to_string();
        let account_name = account_title.replace(' ', "-").to_lowercase();

        tpara!(
            self,
            "\n\
        \"Phishing hint\" is a secret word or a phrase that is displayed \
        when you open your wallet. If you do not see the hint when opening \
        your wallet, you may be accessing a fake wallet designed to steal \
        your private key. If this occurs, stop using the wallet immediately, \
        check the browser URL domain name and seek help on social networks \
        (Kaspa Discord or Telegram). \
        \n\
        ",
        );
        // log_info!("");
        // log_info!("\"Phishing hint\" is a secret word or a phrase that is displayed when you open your wallet.");
        // log_info!("If you do not see the hint when opening your wallet, you may be accessing a fake wallet designed to steal your private key.");
        // log_info!("If this occurs, stop using the wallet immediately, check the domain name and seek help on social networks (Kaspa Discord or Telegram).");
        // log_info!("");
        let hint = term.ask(false, "Create phishing hint (optional, press <enter> to skip): ").await?.trim().to_string();
        let hint = if hint.is_empty() { None } else { Some(hint) };

        let wallet_secret = Secret::new(term.ask(true, "Enter wallet encryption password: ").await?.trim().as_bytes().to_vec());
        if wallet_secret.as_ref().is_empty() {
            return Err(Error::WalletSecretRequired);
        }
        let wallet_secret_validate =
            Secret::new(term.ask(true, "Re-enter wallet encryption password: ").await?.trim().as_bytes().to_vec());
        if wallet_secret_validate.as_ref() != wallet_secret.as_ref() {
            return Err(Error::WalletSecretMatch);
        }

        tprintln!(self, "");
        tpara!(
            self,
            "\
            PLEASE NOTE: The optional payment password, if provided, will be required to \
            issue transactions. This password will also be required when recovering your wallet \
            in addition to your private key or mnemonic. If you loose this password, you will not \
            be able to use mnemonic to recover your wallet! \
            ",
        );

        let payment_secret = term.ask(true, "Enter payment password (optional): ").await?;
        // let payment_secret = payment_secret.trim();
        let payment_secret =
            if payment_secret.trim().is_empty() { None } else { Some(Secret::new(payment_secret.trim().as_bytes().to_vec())) };

        // let payment_secret = Secret::new(
        //     .as_bytes().to_vec(),
        // );
        if let Some(payment_secret) = payment_secret.as_ref() {
            let payment_secret_validate = Secret::new(
                term.ask(true, "Enter payment (private key encryption) password (optional): ").await?.trim().as_bytes().to_vec(),
            );
            if payment_secret_validate.as_ref() != payment_secret.as_ref() {
                return Err(Error::PaymentSecretMatch);
            }
        }

        // suspend commits for multiple operations
        self.wallet.store().batch().await?;

        let account_kind = AccountKind::Bip32;
        let wallet_args = WalletCreateArgs::new(None, hint, wallet_secret.clone(), true);
        let prv_key_data_args = PrvKeyDataCreateArgs::new(None, wallet_secret.clone(), payment_secret.clone());
        let account_args = AccountCreateArgs::new(account_name, account_title, account_kind, wallet_secret.clone(), payment_secret);
        let descriptor = self.wallet.create_wallet(wallet_args).await?;
        let (prv_key_data_id, mnemonic) = self.wallet.create_prv_key_data(prv_key_data_args).await?;
        let account = self.wallet.create_bip32_account(prv_key_data_id, account_args).await?;

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
        self.wallet.store().flush(&ctx).await?;

        ["", "---", "", "IMPORTANT:", ""].into_iter().for_each(|line| term.writeln(line));

        tpara!(
            self,
            "Your mnemonic phrase allows your to re-create your private key. \
            The person who has access to this mnemonic will have full control of \
            the Kaspa stored in it. Keep your mnemonic safe. Write it down and \
            store it in a safe, preferably in a fire-resistant location. Do not \
            store your mnemonic on this computer or a mobile device. This wallet \
            will never ask you for this mnemonic phrase unless you manually \
            initial a private key recovery. \
            ",
        );

        // descriptor

        ["", "Never share your mnemonic with anyone!", "---", "", "Your default wallet account mnemonic:", mnemonic.phrase()]
            .into_iter()
            .for_each(|line| term.writeln(line));

        term.writeln("");
        if let Some(descriptor) = descriptor {
            term.writeln(format!("Your wallet is stored in: {}", descriptor));
            term.writeln("");
        }

        term.writeln("");
        let receive_address = account.receive_address().await?;
        term.writeln(format!("Your default account deposit address: {}", receive_address));

        Ok(())
    }

    pub(crate) async fn create_account(
        &self,
        prv_key_data_id: PrvKeyDataId,
        account_kind: AccountKind,
        name: Option<&str>,
    ) -> Result<()> {
        let term = self.term();

        if matches!(account_kind, AccountKind::MultiSig) {
            return Err(Error::Custom(
                "MultiSig accounts are not currently supported (will be available in the future version)".to_string(),
            ));
        }

        let (title, name) = if let Some(name) = name {
            (name.to_string(), name.to_string())
        } else {
            let title = term.ask(false, "Please enter account title (optional, press <enter> to skip): ").await?.trim().to_string();
            let name = title.replace(' ', "-").to_lowercase();
            (title, name)
        };

        let wallet_secret = Secret::new(term.ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
        if wallet_secret.as_ref().is_empty() {
            return Err(Error::WalletSecretRequired);
        }

        let prv_key_info = self.wallet.store().as_prv_key_data_store()?.load_key_info(&prv_key_data_id).await?;
        if let Some(keyinfo) = prv_key_info {
            let payment_secret = if keyinfo.is_encrypted() {
                let payment_secret = Secret::new(term.ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec());
                if payment_secret.as_ref().is_empty() {
                    return Err(Error::PaymentSecretRequired);
                } else {
                    Some(payment_secret)
                }
            } else {
                None
            };

            let account_args = AccountCreateArgs::new(name, title, account_kind, wallet_secret, payment_secret);
            let account = self.wallet.create_bip32_account(prv_key_data_id, account_args).await?;

            tprintln!(self, "\naccount created: {}\n", account.get_list_string()?);
            self.wallet.select(Some(&account)).await?;
        } else {
            return Err(Error::KeyDataNotFound);
        }

        Ok(())
    }

    pub async fn account(&self) -> Result<Arc<runtime::Account>> {
        if let Ok(account) = self.wallet.account() {
            Ok(account)
        } else {
            let account = self.select_account().await?;
            self.wallet.select(Some(&account)).await?;
            Ok(account)
        }
    }

    pub async fn prompt_account(&self) -> Result<Arc<runtime::Account>> {
        self.select_account_with_args(false).await
    }

    pub async fn select_account(&self) -> Result<Arc<runtime::Account>> {
        self.select_account_with_args(true).await
    }

    async fn select_account_with_args(&self, autoselect: bool) -> Result<Arc<runtime::Account>> {
        let mut selection = None;

        let mut list_by_key = Vec::<(Arc<PrvKeyDataInfo>, Vec<(usize, Arc<runtime::Account>)>)>::new();
        let mut flat_list = Vec::<Arc<runtime::Account>>::new();

        let mut keys = self.wallet.keys().await?;
        while let Some(key) = keys.try_next().await? {
            let mut prv_key_accounts = Vec::new();
            let mut accounts = self.wallet.accounts(Some(key.id)).await?;
            while let Some(account) = accounts.next().await {
                let account = account?;
                prv_key_accounts.push((flat_list.len(), account.clone()));
                flat_list.push(account.clone());
            }

            list_by_key.push((key.clone(), prv_key_accounts));
        }

        if flat_list.is_empty() {
            return Err(Error::NoAccounts);
        } else if autoselect && flat_list.len() == 1 {
            return Ok(flat_list.pop().unwrap());
        }

        while selection.is_none() {
            tprintln!(self);

            list_by_key.iter().for_each(|(prv_key_data_info, accounts)| {
                println!("• {prv_key_data_info}");

                accounts.iter().for_each(|(seq, account)| {
                    let seq = style(seq.to_string()).cyan();
                    println!("    {seq}: {}", account.get_list_string().unwrap_or_else(|err| panic!("{err}")));
                })
            });

            println!();

            let range = if flat_list.len() > 1 { format!("[{}..{}] ", 0, flat_list.len() - 1) } else { "".to_string() };

            let text =
                self.term().ask(false, &format!("Please select account {}or <enter> to abort: ", range)).await?.trim().to_string();
            if text.is_empty() {
                return Err(Error::UserAbort);
            } else {
                match text.parse::<usize>() {
                    Ok(seq) if seq < flat_list.len() => selection = flat_list.get(seq).cloned(),
                    _ => {}
                };
            }
        }

        let account = selection.unwrap();
        let ident = account.name_or_id();
        println!("\nselecting account: {ident}\n");

        Ok(account)
    }

    async fn list(&self) -> Result<()> {
        let mut keys = self.wallet.keys().await?;

        println!();
        while let Some(key) = keys.try_next().await? {
            println!("• {key}");
            let mut accounts = self.wallet.accounts(Some(key.id)).await?;
            while let Some(account) = accounts.try_next().await? {
                let receive_address = account.receive_address().await?;
                println!("    • {}", account.get_list_string()?);
                println!("      {}", style(receive_address.to_string()).yellow());
            }
        }
        println!();

        Ok(())
    }

    // async fn get_credentials(&self) -> Result<Secret> {
    // }
}

#[async_trait]
impl Cli for WalletCli {
    fn init(&self, term: &Arc<Terminal>) -> TerminalResult<()> {
        *self.term.lock().unwrap() = Some(term.clone());
        // *self.context.lock().unwrap() = Some(Arc::new(context::Wallet::new(&term, &self.wallet)));

        Ok(())
    }

    async fn digest(self: Arc<Self>, _term: Arc<Terminal>, cmd: String) -> TerminalResult<()> {
        *self.last_interaction.lock().unwrap() = Instant::now();

        // let ctx = Arc::new(Context{});
        // self.ctx
        self.handlers.execute(&self, &cmd).await?;

        // let argv = parse(&cmd);
        // let action: Action = argv[0].as_str().try_into()?;
        // self.action(action, argv, term, cmd.as_str()).await?;
        Ok(())
    }

    async fn complete(self: Arc<Self>, _term: Arc<Terminal>, _cmd: String) -> TerminalResult<Vec<String>> {
        // TODO
        // let argv = parse(&cmd);
        Ok(vec![])
        // if argv.len() == 1 {
        //     // let part = argv.first().unwrap().as_str();
        //     // let mut list = vec![];
        //     // for (cmd,_) in HELP.iter() {
        //     //     if cmd.starts_with(part) {
        //     //         list.push(cmd.to_string());
        //     //     }
        //     // };
        //     // Ok(list)
        //     Ok(vec![])
        // } else {
        //     Ok(vec![])
        // }
    }

    fn prompt(&self) -> Option<String> {
        if let Some(name) = self.wallet.name() {
            if let Ok(account) = self.wallet.account() {
                let ident = account.name_or_id();
                Some(format!("{name} • {ident} $ "))
            } else {
                Some(format!("{name} $ "))
            }
        } else {
            None
        }
        // self.wallet.account().ok().map(|account| format!("{name}{} $ ", account.name_or_id()))
    }
}

impl cli::Context for WalletCli {
    fn term(&self) -> Arc<Terminal> {
        self.term.lock().unwrap().as_ref().unwrap().clone()
    }
}

impl WalletCli {}

use kaspa_wallet_core::runtime::{self, BalanceStrings, PrvKeyDataCreateArgs};

#[allow(dead_code)]
async fn select_item<T>(
    term: &Arc<Terminal>,
    prompt: &str,
    argv: &mut Vec<String>,
    iter: impl Stream<Item = Result<Arc<T>>>,
) -> Result<Arc<T>>
where
    T: std::fmt::Display + IdT + Clone + Send + Sync + 'static,
{
    let mut selection = None;
    let list = iter.try_collect::<Vec<_>>().await?;

    if !argv.is_empty() {
        let text = argv.remove(0);
        let matched = list
            .into_iter()
            // - TODO match by name
            .filter(|item| item.id().to_hex().starts_with(&text))
            .collect::<Vec<_>>();

        if matched.len() == 1 {
            return Ok(matched.first().cloned().unwrap());
        } else {
            return Err(Error::MultipleMatches(text));
        }
    }

    while selection.is_none() {
        list.iter().enumerate().for_each(|(seq, item)| {
            term.writeln(format!("{}: {} ({})", seq, item, item.id().to_hex()));
        });

        let text = term.ask(false, &format!("{prompt} ({}..{}) or <enter> to abort: ", 0, list.len() - 1)).await?.trim().to_string();
        if text.is_empty() {
            term.writeln("aborting...");
            return Err(Error::UserAbort);
        } else {
            match text.parse::<usize>() {
                Ok(seq) if seq < list.len() => selection = list.get(seq).cloned(),
                _ => {}
            };
        }
    }

    Ok(selection.unwrap())
}

// async fn select_variant<T>(term: &Arc<Terminal>, prompt: &str, argv: &mut Vec<String>) -> Result<T>
// where
//     T: ToString + DeserializeOwned + Clone + Serialize,
// {
//     if !argv.is_empty() {
//         let text = argv.remove(0);
//         if let Ok(v) = serde_json::from_str::<T>(text.as_str()) {
//             return Ok(v);
//         } else {
//             let accepted = T::list().iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join(", ");
//             return Err(Error::UnrecognizedArgument(text, accepted));
//         }
//     }

//     let mut selection = None;
//     let list = T::list();
//     while selection.is_none() {
//         list.iter().enumerate().for_each(|(seq, item)| {
//             let name = serde_json::to_string(item).unwrap();
//             term.writeln(format!("{}: '{name}' - {}", seq, item.descr()));
//         });

//         let text = term.ask(false, &format!("{prompt} ({}..{}) or <enter> to abort: ", 0, list.len() - 1)).await?.trim().to_string();
//         if text.is_empty() {
//             term.writeln("aborting...");
//             return Err(Error::UserAbort);
//         } else if let Ok(v) = serde_json::from_str::<T>(text.as_str()) {
//             selection = Some(v);
//         } else {
//             match text.parse::<usize>() {
//                 Ok(seq) if seq > 0 && seq < list.len() => selection = list.get(seq).cloned(),
//                 _ => {}
//             };
//         }
//     }

//     Ok(selection.unwrap())
// }

pub async fn kaspa_cli(options: TerminalOptions, banner: Option<String>) -> Result<()> {
    cfg_if! {
        if #[cfg(not(target_arch = "wasm32"))] {
            init_panic_hook(||{
                std::println!("halt");
                1
            });
            kaspa_core::log::init_logger(None, "info");
        }
    }

    workflow_log::set_colors_enabled(true);

    let wallet = Arc::new(Wallet::try_new(Wallet::local_store()?, None)?);
    let cli = Arc::new(WalletCli::new(wallet.clone()));
    let term = Arc::new(Terminal::try_new_with_options(cli.clone(), options)?);
    term.init().await?;

    // redirect the global log output to terminal
    #[cfg(not(target_arch = "wasm32"))]
    workflow_log::pipe(Some(cli.clone()));

    // cli starts notification->term trace pipe task
    cli.start().await?;

    // ----------------------------------------------------------------------

    // let mut modules: Vec<Arc<dyn Handler>> = vec![
    //     // Arc::new(help::Help),
    //     // Arc::new(address::Address),
    // ];

    register_handlers!(
        cli,
        cli.handlers,
        [
            address,
            // broadcast,
            close,
            connect,
            // create_unsigned_tx,
            create,
            details,
            disconnect,
            estimate,
            exit,
            export,
            halt,
            help,
            hint,
            import,
            info,
            list,
            metrics,
            mute,
            name,
            network,
            new_address,
            open,
            ping,
            reload,
            select,
            send,
            server,
            set,
            // sign,
            // sweep,
            track,
        ]
    );

    // let modules = vec![
    //     Box::new(&help::Help::default()),
    //     Box::new(&address::Address::default())
    // ];

    // modules.into_iter().for_each(|module| {
    //     cli.handlers.register_arc(&cli,&module);
    // });

    // cli.handlers.register(&cli,TestHandler::default());
    // cli.handlers.register(&cli,help::Help::default());
    // cli.handlers.register(&cli,address::Address::default());
    // cli.handlers.register(&cli,address::Address::default());

    // let ctx = Arc::new(crate::context::Wallet::new(&term, &wallet));
    cli.handlers.start(&cli).await?;

    // ----------------------------------------------------------------------

    // unsafe {
    //     kaspa_cli::TERMINAL = Some(term.clone());
    // }

    let banner =
        banner.unwrap_or_else(|| format!("Kaspa Cli Wallet v{} (type 'help' for list of commands)", env!("CARGO_PKG_VERSION")));
    term.writeln(banner);

    // wallet starts rpc and notifier
    wallet.start().await?;
    // terminal blocks async execution, delivering commands to the terminals
    term.run().await?;

    cli.handlers.stop(&cli).await?;

    // wallet stops the notifier
    wallet.stop().await?;
    // cli stops notification->term trace pipe task
    cli.stop().await?;

    // unsafe {
    //     kaspa_cli::TERMINAL = None;
    // }

    Ok(())
}
