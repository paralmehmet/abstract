use crate::dapp_base::common::TEST_CREATOR;
use abstract_os::core::proxy::msg as TreasuryMsg;
use abstract_os::native::memory::msg as MemoryMsg;
use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::{PairConfig, PairType};
use cosmwasm_std::testing::{mock_env, MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{attr, Addr, Empty, Timestamp, Uint128};
use cw_multi_test::{App, BankKeeper, ContractWrapper, Executor};
use terra_mocks::TerraMockQuerier;

pub struct BaseContracts {
    pub whale: Addr,
    pub memory: Addr,
    pub proxy: Addr,
    pub whale_ust_pair: Addr,
    pub whale_ust: Addr,
}

/// Creates the basic contract instances needed to test the dapp.
/// Whale token, Memory, Treasury, Whale/UST pair, Whale/UST LP
pub fn init_contracts(app: &mut App) -> BaseContracts {
    let owner = Addr::unchecked(TEST_CREATOR);

    // Instantiate WHALE Token Contract
    let cw20_token_contract = Box::new(ContractWrapper::new(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    ));

    let cw20_token_code_id = app.store_code(cw20_token_contract);

    let msg = cw20_base::msg::InstantiateMsg {
        name: String::from("Whale token"),
        symbol: String::from("WHALE"),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(cw20::MinterResponse {
            minter: owner.to_string(),
            cap: None,
        }),
        marketing: None,
    };

    let whale_token_instance = app
        .instantiate_contract(
            cw20_token_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("WHALE"),
            None,
        )
        .unwrap();

    // Upload Treasury Contract
    let proxy_contract = Box::new(ContractWrapper::new(
        proxy::contract::execute,
        proxy::contract::instantiate,
        proxy::contract::query,
    ));

    let proxy_code_id = app.store_code(proxy_contract);

    let proxy_instantiate_msg = TreasuryMsg::InstantiateMsg {};

    // Instantiate Treasury Contract
    let proxy_instance = app
        .instantiate_contract(
            proxy_code_id,
            owner.clone(),
            &proxy_instantiate_msg,
            &[],
            "Treasury",
            None,
        )
        .unwrap();

    // Upload Memory Contract
    let memory_contract = Box::new(ContractWrapper::new(
        memory::contract::execute,
        memory::contract::instantiate,
        memory::contract::query,
    ));

    let memory_code_id = app.store_code(memory_contract);

    let memory_instantiate_msg = MemoryMsg::InstantiateMsg {};

    // Init contract
    let memory_instance = app
        .instantiate_contract(
            memory_code_id,
            owner.clone(),
            &memory_instantiate_msg,
            &[],
            "Memory",
            None,
        )
        .unwrap();

    // Instantiate the terraswap pair
    let (pair, lp) = instantiate_pair(app, &owner.clone(), &whale_token_instance);

    app.update_block(|b| {
        b.height += 17;
        b.time = Timestamp::from_seconds(1571797419);
    });

    BaseContracts {
        proxy: proxy_instance,
        memory: memory_instance,
        whale: whale_token_instance,
        whale_ust_pair: pair,
        whale_ust: lp,
    }
}

pub fn mock_app() -> App<Empty> {
    let env = mock_env();
    let api = MockApi::default();
    let bank = BankKeeper::new();
    let custom_querier: TerraMockQuerier =
        TerraMockQuerier::new(MockQuerier::new(&[(MOCK_CONTRACT_ADDR, &[])]));

    App::new(api, env.block, bank, MockStorage::new(), custom_querier)
    // let custom_handler = CachingCustomHandler::<CustomMsg, Empty>::new();
    // AppBuilder::new().with_custom(custom_handler).build()
}

/// Create terraswap WHALE/UST pair
fn instantiate_pair(
    mut router: &mut App,
    owner: &Addr,
    whale_token_instance: &Addr,
) -> (Addr, Addr) {
    let token_contract_code_id = store_token_code(&mut router);

    let pair_contract_code_id = store_pair_code(&mut router);

    let factory_contract_code_id = store_factory_code(&mut router);

    let factory_msg = astroport::factory::InstantiateMsg {
        fee_address: None,
        generator_address: None,
        owner: owner.to_string(),
        pair_configs: vec![PairConfig {
            code_id: pair_contract_code_id,
            pair_type: PairType::Xyk {},
            total_fee_bps: 10u16,
            maker_fee_bps: 10u16,
            is_disabled: None,
        }],
        token_code_id: token_contract_code_id,
    };

    let factory = router
        .instantiate_contract(
            factory_contract_code_id,
            owner.clone(),
            &factory_msg,
            &[],
            String::from("Factory"),
            None,
        )
        .unwrap();

    let msg = astroport::pair::InstantiateMsg {
        asset_infos: [
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: whale_token_instance.clone(),
            },
        ],
        token_code_id: token_contract_code_id,
        factory_addr: factory,
        init_params: None,
    };

    let pair = router
        .instantiate_contract(
            pair_contract_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("PAIRRR"),
            None,
        )
        .unwrap();

    let res: PairInfo = router
        .wrap()
        .query_wasm_smart(pair.clone(), &astroport::pair::QueryMsg::Pair {})
        .unwrap();
    assert_eq!("Contract #4", res.contract_addr.to_string());
    assert_eq!("Contract #5", res.liquidity_token.to_string());

    (pair, Addr::unchecked(res.liquidity_token))
}

/// Whitelist a dapp on the proxy
pub fn whitelist_dapp(app: &mut App, owner: &Addr, proxy_instance: &Addr, dapp_instance: &Addr) {
    let msg = TreasuryMsg::ExecuteMsg::AddDApp {
        dapp: dapp_instance.to_string(),
    };
    let _res = app
        .execute_contract(owner.clone(), proxy_instance.clone(), &msg, &[])
        .unwrap();
    // Check if it was added
    let resp: TreasuryMsg::ConfigResponse = app
        .wrap()
        .query_wasm_smart(proxy_instance, &TreasuryMsg::QueryMsg::Config {})
        .unwrap();

    // Check config
    assert!(resp.dapps.contains(&dapp_instance.to_string()));
}

/// Mint Whale tokens
pub fn mint_some_whale(
    app: &mut App,
    owner: Addr,
    whale_token_instance: Addr,
    amount: Uint128,
    to: String,
) {
    let msg = cw20::Cw20ExecuteMsg::Mint {
        recipient: to.clone(),
        amount,
    };
    let res = app
        .execute_contract(owner.clone(), whale_token_instance.clone(), &msg, &[])
        .unwrap();
    assert_eq!(res.events[1].attributes[1], attr("action", "mint"));
    assert_eq!(res.events[1].attributes[2], attr("to", to));
    assert_eq!(res.events[1].attributes[3], attr("amount", amount));
}

fn store_token_code(app: &mut App) -> u64 {
    let whale_token_contract = Box::new(ContractWrapper::new(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    app.store_code(whale_token_contract)
}

fn store_pair_code(app: &mut App) -> u64 {
    let pair_contract = Box::new(
        ContractWrapper::new(
            astroport_pair::contract::execute,
            astroport_pair::contract::instantiate,
            astroport_pair::contract::query,
        )
        .with_reply(astroport_pair::contract::reply),
    );

    app.store_code(pair_contract)
}

fn store_factory_code(app: &mut App) -> u64 {
    let factory_contract = Box::new(
        ContractWrapper::new(
            astroport_factory::contract::execute,
            astroport_factory::contract::instantiate,
            astroport_factory::contract::query,
        )
        .with_reply(astroport_factory::contract::reply),
    );

    app.store_code(factory_contract)
}
