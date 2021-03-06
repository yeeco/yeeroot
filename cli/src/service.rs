//! Service and ServiceFactory implementation. Specialized wrapper over Substrate service.

#![warn(unused_extern_crates)]

use std::time::Duration;
use std::sync::Arc;
use log::info;
use transaction_pool::{self, txpool::{Pool as TransactionPool}};
use substrate_service::{
    FactoryFullConfiguration, LightComponents, FullComponents, FullBackend,
    FullClient, LightClient, LightBackend, FullExecutor, LightExecutor,
    TaskExecutor,
};
use basic_authorship::ProposerFactory;
use substrate_client as client;
use primitives::{ed25519::Pair, Pair as PairT, H256};
use inherents::InherentDataProviders;
use network::{construct_simple_protocol};
use substrate_executor::native_executor_instance;
use substrate_service::construct_service_factory;
use substrate_service::config::Roles;
use {
    parking_lot::RwLock,
    consensus::{self, import_queue, start_pow, PowImportQueue, JobManager, DefaultJob},
    consensus_common::import_queue::{ImportQueue, BlockBuilder},
    foreign_chain::{ForeignChain, ForeignChainConfig},
    substrate_service::{
        NetworkProviderParams, FactoryBlock, NetworkProvider, Network, ServiceFactory, ComponentExHash,
    },
    yee_runtime::{
        self, GenesisConfig, opaque::Block, RuntimeApi,
        AccountId,
    },
    yee_rpc::{FullRpcHandlerConstructor, LightRpcHandlerConstructor},
    yee_sharding::identify_specialization::ShardingIdentifySpecialization,
};
use substrate_cli::{TriggerExit};
use sharding_primitives::ScaleOut;
use runtime_primitives::traits::{Block as BlockT, NumberFor};
use futures::sync::mpsc;
use yee_primitives::{RecommitRelay, AddressCodec};
use std::collections::HashMap;

mod foreign;
use foreign::{start_foreign_network};

mod restarter;
use restarter::{start_restarter};

pub use substrate_executor::NativeExecutor;
use yee_bootnodes_router::BootnodesRouterConf;
use yee_rpc::{ProvideRpcExtra, Config};

use crfg;
use yee_primitives::Hrp;
use crate::{CliTriggerExit, CliSignal};
use yee_context::{Context};
use crfg::CrfgStateProvider;
use yee_foreign_network::SyncProvider;
use crate::custom_param::NodeKeyParams;

pub const IMPL_NAME : &str = "yee-node";
pub const NATIVE_PROTOCOL_VERSION : &str = "/yee/1.0.0";
pub const FOREIGN_PROTOCOL_VERSION : &str = "/yee-foreign/1.0.0";

#[cfg(feature = "custom-wasm-code")]
pub const WASM_CODE: &'static [u8] = include_bytes!(env!("WASM_CODE_PATH"));

#[cfg(not(feature = "custom-wasm-code"))]
pub const WASM_CODE: &'static [u8] = include_bytes!("../../runtime/wasm/target/wasm32-unknown-unknown/release/yee_runtime_wasm.compact.wasm");

// Our native executor instance.
native_executor_instance!(
    pub Executor,
    yee_runtime::api::dispatch,
    yee_runtime::native_version,
    WASM_CODE
);

/// Node specific configuration
pub struct NodeConfig<F: substrate_service::ServiceFactory> {
    /// crfg connection to import block
    // FIXME #1134 rather than putting this on the config, let's have an actual intermediate setup state
    pub crfg_import_setup: Option<(Arc<PowImportQueue<F::Block>>, crfg::LinkHalfForService<F>)>,
    pub inherent_data_providers: InherentDataProviders,
    pub coinbase: Option<AccountId>,
    pub shard_num: u16,
    pub shard_count: u16,
    pub foreign_port: Option<u16>,
    pub foreign_out_peers: u32,
    pub foreign_in_peers: u32,
    pub foreign_node_key_params: NodeKeyParams,
    pub bootnodes_router_conf: Option<BootnodesRouterConf>,
    pub job_manager: Arc<RwLock<Option<Arc<dyn JobManager<Job=DefaultJob<Block, <Pair as PairT>::Public>>>>>>,
    pub recommit_relay_sender: Arc<RwLock<Option<mpsc::UnboundedSender<RecommitRelay<<F::Block as BlockT>::Hash>>>>>,
    pub crfg_state_provider: Arc<RwLock<Option<Arc<dyn CrfgStateProvider<<F::Block as BlockT>::Hash, NumberFor<F::Block>>>>>>,
    pub import_crfg_state_providers: Arc<RwLock<HashMap<u16, Arc<dyn CrfgStateProvider<<F::Block as BlockT>::Hash, NumberFor<F::Block>>>>>>,
    pub mine: bool,
    pub import_until: Option<HashMap<u16, NumberFor<F::Block>>>,
    pub import_leading: Option<NumberFor<F::Block>>,
    pub job_cache_size: Option<u32>,
    pub foreign_chains: Arc<RwLock<Option<ForeignChain<F>>>>,
    pub foreign_network: Arc<RwLock<Option<Arc<dyn SyncProvider<F::Block, ComponentExHash<FullComponents<F>>>>>>>,
    pub hrp: Hrp,
    pub scale_out: Option<ScaleOut>,
    pub trigger_exit: Option<Arc<dyn consensus::TriggerExit>>,
    pub context: Option<Context<F::Block>>,
}

impl<F: substrate_service::ServiceFactory> Default for NodeConfig<F> {
    fn default() -> Self {
        Self {
            crfg_import_setup: None,
            inherent_data_providers: Default::default(),
            coinbase: Default::default(),
            shard_num: Default::default(),
            shard_count: Default::default(),
            foreign_port: Default::default(),
            foreign_out_peers: Default::default(),
            foreign_in_peers: Default::default(),
            foreign_node_key_params: Default::default(),
            bootnodes_router_conf: Default::default(),
            job_manager: Arc::new(RwLock::new(None)),
            recommit_relay_sender: Arc::new(RwLock::new(None)),
            crfg_state_provider: Arc::new(RwLock::new(None)),
            import_crfg_state_providers: Arc::new(RwLock::new(HashMap::new())),
            mine: Default::default(),
            import_until: Default::default(),
            import_leading: Default::default(),
            job_cache_size: Default::default(),
            foreign_chains: Arc::new(RwLock::new(None)),
            foreign_network: Arc::new(RwLock::new(None)),
            hrp: Default::default(),
            scale_out: Default::default(),
            trigger_exit: Default::default(),
            context: Default::default(),
        }
    }
}

impl<F: substrate_service::ServiceFactory> Clone for NodeConfig<F> {
    fn clone(&self) -> Self {
        Self {
            crfg_import_setup: None,
            coinbase: self.coinbase.clone(),
            shard_num: self.shard_num,
            shard_count: self.shard_count,
            foreign_port: self.foreign_port,
            foreign_out_peers: self.foreign_out_peers,
            foreign_in_peers: self.foreign_in_peers,
            foreign_node_key_params: self.foreign_node_key_params.clone(),
            mine: self.mine,
            import_until: self.import_until.clone(),
            import_leading: self.import_leading.clone(),
            job_cache_size: self.job_cache_size,
            hrp: self.hrp.clone(),
            scale_out: self.scale_out.clone(),
            trigger_exit: self.trigger_exit.clone(),
            context: self.context.clone(),
            job_manager: self.job_manager.clone(),
            recommit_relay_sender: self.recommit_relay_sender.clone(),
            crfg_state_provider: self.crfg_state_provider.clone(),
            import_crfg_state_providers: self.import_crfg_state_providers.clone(),
            foreign_chains: self.foreign_chains.clone(),
            foreign_network: self.foreign_network.clone(),

            // cloned config SHALL NOT SHARE some items with original config
            inherent_data_providers: Default::default(),
            bootnodes_router_conf: None,
        }
    }
}

impl<F> ForeignChainConfig for NodeConfig<F> where F: substrate_service::ServiceFactory {
    fn get_shard_num(&self) -> u16 {
        self.shard_num
    }

    fn set_shard_num(&mut self, shard: u16) {
        self.shard_num = shard;
    }

    fn get_shard_count(&self) -> u16 {
        self.shard_count
    }
}

impl<F> ProvideRpcExtra<DefaultJob<Block, <Pair as PairT>::Public>, F::Block, ComponentExHash<FullComponents<F>>> for NodeConfig<F> where
    F: substrate_service::ServiceFactory,
{
    fn provide_job_manager(&self) -> Arc<RwLock<Option<Arc<dyn JobManager<Job=DefaultJob<Block, <Pair as PairT>::Public>>>>>>{
        self.job_manager.clone()
    }

    fn provide_recommit_relay_sender(&self) -> Arc<RwLock<Option<mpsc::UnboundedSender<RecommitRelay<<F::Block as BlockT>::Hash>>>>> {
        self.recommit_relay_sender.clone()
    }

    fn provide_crfg_state_provider(&self) -> Arc<RwLock<Option<Arc<dyn CrfgStateProvider<<F::Block as BlockT>::Hash, NumberFor<F::Block>>>>>> {
        self.crfg_state_provider.clone()
    }

    fn provide_import_crfg_state_providers(&self) -> Arc<RwLock<HashMap<u16, Arc<dyn CrfgStateProvider<<F::Block as BlockT>::Hash, NumberFor<F::Block>>>>>>{
        self.import_crfg_state_providers.clone()
    }

    fn provide_foreign_network(&self) -> Arc<RwLock<Option<Arc<dyn SyncProvider<F::Block, ComponentExHash<FullComponents<F>>>>>>> {
        self.foreign_network.clone()
    }

    fn provide_config(&self) -> Arc<Config> {
        let hrp = self.hrp.clone();
        let coinbase = self.coinbase.as_ref().map(|x|x.to_address(hrp.clone()).expect("qed").0);
        let job_cache_size = self.job_cache_size;
        let shard_num = self.shard_num;
        let shard_count = self.shard_count;
        let config = Config {
            shard_num,
            shard_count,
            coinbase,
            job_cache_size,
        };
        Arc::new(config)
    }

}

struct NetworkWrapper<F, EH> {
    inner: Arc<dyn NetworkProvider<F, EH>>,
}

impl<F, EH> Clone for NetworkWrapper<F, EH> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<F, EH> NetworkProvider<F, EH> for NetworkWrapper<F, EH> where
    F: ServiceFactory,
    EH: network::service::ExHashT,
{
    fn provide_network(
        &self,
        network_id: u32,
        params: NetworkProviderParams<F, EH>,
        protocol_id: network::ProtocolId,
        import_queue: Box<dyn ImportQueue<FactoryBlock<F>>>,
    ) -> Result<(Arc<dyn Network<F::Block>>, network::NetworkChan<FactoryBlock<F>>), network::Error> {
        self.inner.provide_network(network_id, params, protocol_id, import_queue)
    }
}

impl consensus::TriggerExit for CliTriggerExit<CliSignal>{
    fn trigger_restart(&self){
        self.trigger_exit(CliSignal::Restart);
    }

    fn trigger_stop(&self){
        self.trigger_exit(CliSignal::Stop);
    }
}

construct_simple_protocol! {
    /// Demo protocol attachment for substrate.
    pub struct NodeProtocol where Block = Block { }
}

construct_service_factory! {
    struct Factory {
        Block = Block,
        RuntimeApi = RuntimeApi,
        NetworkProtocol = NodeProtocol { |config| Ok(NodeProtocol::new()) },
        RuntimeDispatch = Executor,
        FullTransactionPoolApi = transaction_pool::ChainApi<client::Client<FullBackend<Self>, FullExecutor<Self>, Block, RuntimeApi>, Block>
            { |config, client| Ok(TransactionPool::new(config, transaction_pool::ChainApi::new(client))) },
        LightTransactionPoolApi = transaction_pool::ChainApi<client::Client<LightBackend<Self>, LightExecutor<Self>, Block, RuntimeApi>, Block>
            { |config, client| Ok(TransactionPool::new(config, transaction_pool::ChainApi::new(client))) },
        Genesis = GenesisConfig,
        Configuration = NodeConfig<Self>,
        FullService = FullComponents<Self>
            { |config: FactoryFullConfiguration<Self>, executor: TaskExecutor|
                FullComponents::<Factory>::new(config, executor)
            },
        AuthoritySetup = {
            |mut service: Self::FullService, executor: TaskExecutor, key: Option<Arc<Pair>>, next_key: Option<Arc<Pair>>| {
                let (sender, receiver): (mpsc::UnboundedSender<RecommitRelay<H256>>, mpsc::UnboundedReceiver<RecommitRelay<H256>>) = mpsc::unbounded();

                // foreign network
                // let config = &service.config;
                let foreign_network_param = foreign::Params{
                    client_version: service.config.network.client_version.clone(),
                    protocol_version : FOREIGN_PROTOCOL_VERSION.to_string(),
                    shard_num: service.config.custom.shard_num,
                    shard_count: service.config.custom.shard_count,
                    foreign_port: service.config.custom.foreign_port,
                    foreign_out_peers: service.config.custom.foreign_out_peers,
                    foreign_in_peers:  service.config.custom.foreign_in_peers,
                    foreign_node_key_params: service.config.custom.foreign_node_key_params.clone(),
                    net_config_path: service.config.network.net_config_path.clone(),
                    bootnodes_router_conf: service.config.custom.bootnodes_router_conf.clone(),
                };
                let foreign_network = start_foreign_network::<FullComponents<Self>>(foreign_network_param, service.client(), &executor).map_err(|e| format!("{:?}", e))?;

                // foreign chain
                let foreign_network_wrapper = NetworkWrapper { inner: foreign_network.clone()};
                let foreign_chain = ForeignChain::<Self>::new(
                    &service.config,
                    foreign_network_wrapper,
                    executor.clone(),
                )?;
                {
                    let mut config_foreign_chains = service.config.custom.foreign_chains.write();
                    *config_foreign_chains = Some(foreign_chain);

                    let mut recommit_relay_sender = service.config.custom.recommit_relay_sender.write();
                    *recommit_relay_sender = Some(sender);

                    let mut config_foreign_network = service.config.custom.foreign_network.write();
                    *config_foreign_network = Some(foreign_network.clone());
                }


                // relay
                yee_relay::start_relay_transfer::<Self, _, _>(
                    service.client(),
                    &executor,
                    foreign_network.clone(),
                    service.config.custom.foreign_chains.clone(),
                    service.transaction_pool(),
                    receiver
                ).map_err(|e| format!("{:?}", e))?;

                // restarter
                let restarter_param = restarter::Params{
                    authority_id: next_key.clone().map(|k|k.public()),
                    coinbase: service.config.custom.coinbase.clone(),
                    shard_num: service.config.custom.shard_num,
                    shard_count: service.config.custom.shard_count,
                    scale_out: service.config.custom.scale_out.clone(),
                    trigger_exit: service.config.custom.trigger_exit.clone().expect("qed"),
                };
                start_restarter::<FullComponents<Self>>(restarter_param, service.client(), &executor);

                // validator
                let validator = service.config.roles == Roles::AUTHORITY;

                if validator {

                    let worker_key = if next_key.is_some() { next_key.clone() } else { key.clone() };
                    let worker_key = worker_key.expect("qed");

                    info!("Running crfg session as Authority: {:?}, next: {:?}", key.as_ref().map(|x|x.public()), next_key.as_ref().map(|x|x.public()));

                    // crfg
                    let (block_builder, link_half) = service.config.custom.crfg_import_setup.take()
                    .expect("Link Half and Block Import are present for Full Services or setup failed before. qed");

                    crfg::register_crfg_inherent_data_provider(
                        &service.config.custom.inherent_data_providers.clone(),
                        worker_key.clone().public()
                    )?;

                    executor.spawn(crfg::run_crfg(
                        crfg::Config {
                            local_key: key,
                            local_next_key: next_key,
                            // FIXME #1578 make this available through chainspec
                            gossip_duration: Duration::from_millis(333),
                            justification_period: 4096,
                            name: Some(service.config.name.clone())
                        },
                        link_half,
                        crfg::NetworkBridge::new(service.network()),
                        service.config.custom.inherent_data_providers.clone(),
                        service.on_exit(),
                        service.config.custom.crfg_state_provider.clone(),
                    )?);

                    // pow
                    let proposer = Arc::new(ProposerFactory {
                        client: service.client(),
                        transaction_pool: service.transaction_pool(),
                        inherents_pool: service.inherents_pool(),
                    });
                    let client = service.client();

                    let params = consensus::Params{
                        force_authoring: service.config.force_authoring,
                        mine: service.config.custom.mine,
                        shard_extra: consensus::ShardExtra {
                            coinbase: service.config.custom.coinbase.clone(),
                            shard_num: service.config.custom.shard_num,
                            shard_count: service.config.custom.shard_count,
                            scale_out: service.config.custom.scale_out.clone(),
                            trigger_exit: service.config.custom.trigger_exit.clone().expect("qed"),
                        },
                        context: service.config.custom.context.clone().expect("qed"),
                        chain_spec_id: service.config.chain_spec.id().to_string(),
                    };

                    executor.spawn(start_pow::<Self, Self::Block, _, _, _, _, _, _, _>(
                        worker_key.clone(),
                        client.clone(),
                        block_builder.clone(),
                        proposer,
                        service.network(),
                        service.on_exit(),
                        service.config.custom.inherent_data_providers.clone(),
                        service.config.custom.job_manager.clone(),
                        params,
                        service.config.custom.foreign_chains.clone(),
                    )?);
                }

                Ok(service)
            }
        },
        LightService = LightComponents<Self>
            { |config, executor| <LightComponents<Factory>>::new(config, executor) },
        FullImportQueue = PowImportQueue<Self::Block>
            { |config: &mut FactoryFullConfiguration<Self> , client: Arc<FullClient<Self>>| {

                    let validator = config.roles == Roles::AUTHORITY;
                    let shard_num = config.custom.shard_num;
                    let import_until = config.custom.import_until.as_ref().and_then(|x| x.get(&shard_num).cloned());
                    let import_leading = config.custom.import_leading;

                    let (block_import, link_half) = crfg::block_import::<_, _, _, RuntimeApi, FullClient<Self>>(
                        client.clone(), client.clone(), validator, import_until, import_leading,
                        config.chain_spec.id().to_string(),
                        config.custom.shard_num,
                        config.custom.import_crfg_state_providers.clone(),
                    )?;

                    let block_import = Arc::new(block_import);
                    let justification_import = block_import.clone();

                    info!("Start full import queue, shard_num: {}", config.custom.shard_num);
                    let import_queue = import_queue::<Self, _,  _, <Pair as PairT>::Public>(
                        block_import,
                        Some(justification_import),
                        client,
                        config.custom.inherent_data_providers.clone(),
                        config.custom.foreign_chains.clone(),
                        consensus::ShardExtra {
                            coinbase: config.custom.coinbase.clone(),
                            shard_num: config.custom.shard_num,
                            shard_count: config.custom.shard_count,
                            scale_out: config.custom.scale_out.clone(),
                            trigger_exit: config.custom.trigger_exit.clone().expect("qed"),
                        },
                        config.custom.context.clone().expect("qed"),
                        config.chain_spec.id().to_string(),
                        true,
                    ).expect("qed");

                    if validator {
                        config.custom.crfg_import_setup = Some((Arc::new(import_queue.clone()), link_half));
                    }

                    Ok(import_queue)
                }
            },
        LightImportQueue = PowImportQueue<Self::Block>
            { |config: &mut FactoryFullConfiguration<Self>, client: Arc<LightClient<Self>>| {

                    let shard_num = config.custom.shard_num;
                    let import_until = config.custom.import_until.as_ref().and_then(|x| x.get(&shard_num).cloned());
                    let import_leading = config.custom.import_leading;

                    let (block_import, _) = crfg::block_import::<_, _, _, RuntimeApi, LightClient<Self>>(
                        client.clone(), client.clone(), false, import_until, import_leading,
                        config.chain_spec.id().to_string(),
                        config.custom.shard_num,
                        config.custom.import_crfg_state_providers.clone(),
                    )?;

                    let block_import = Arc::new(block_import);
                    let justification_import = block_import.clone();

                    info!("Start light import queue, shard_num: {}", config.custom.shard_num);
                    import_queue::<Self, _,  _, <Pair as PairT>::Public>(
                        block_import,
                        Some(justification_import),
                        client,
                        config.custom.inherent_data_providers.clone(),
                        Arc::new(RwLock::new(None)),
                        consensus::ShardExtra {
                            coinbase: config.custom.coinbase.clone(),
                            shard_num: config.custom.shard_num,
                            shard_count: config.custom.shard_count,
                            scale_out: config.custom.scale_out.clone(),
                            trigger_exit: config.custom.trigger_exit.clone().expect("qed"),
                        },
                        config.custom.context.clone().expect("qed"),
                        config.chain_spec.id().to_string(),
                        false,
                    ).map_err(Into::into)

                    // import_queue::<Self, _,  _, <Pair as PairT>::Public>(
                    //     client.clone(),
                    //     None,
                    //     client,
                    //     config.custom.inherent_data_providers.clone(),
                    //     Arc::new(RwLock::new(None)),
                    //     consensus::ShardExtra {
                    //         coinbase: config.custom.coinbase.clone(),
                    //         shard_num: config.custom.shard_num,
                    //         shard_count: config.custom.shard_count,
                    //         scale_out: config.custom.scale_out.clone(),
                    //         trigger_exit: config.custom.trigger_exit.clone().expect("qed"),
                    //     },
                    //     config.custom.context.clone().expect("qed"),
                    // ).map_err(Into::into)
                }
            },
        FullRpcHandlerConstructor = FullRpcHandlerConstructor,
        LightRpcHandlerConstructor = LightRpcHandlerConstructor,
        IdentifySpecialization = ShardingIdentifySpecialization
            { |config: &FactoryFullConfiguration<Self>| {
                Ok(ShardingIdentifySpecialization::new(NATIVE_PROTOCOL_VERSION.to_string(), config.custom.shard_num))
                }
            },
    }
}
