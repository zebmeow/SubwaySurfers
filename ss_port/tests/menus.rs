//! The menus around a run, driven headlessly through the same clicks and
//! keys the UI sends: title -> boost shop (upgrade) -> Me panel (select
//! the original roster, buy Tricky and an outfit) -> persisted save -> PLAY -> a run
//! where the upgrade lengthens the magnet and a hoverboard press with no
//! boards left pauses into the buy popup and resumes after a countdown.

use ss_port::flow::{self, Click, Screen, UserData};
use ss_port::game::GameState;
use ss_port::hero::Key;
use ss_port::menu::{self, MeTab, Menu};
use ss_port::sim_plugin::{DataPaths, KeyQueue, Sim};
use std::path::Path;

fn title_sim() -> (Sim, KeyQueue) {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
    let keys = KeyQueue::default();
    let s = Sim::new_title(&DataPaths::from_repo(root), 1, Some("S".into()), Box::new(keys.clone())).unwrap();
    (s, keys)
}

fn steps(s: &mut Sim, n: usize) {
    for _ in 0..n {
        s.step();
    }
}

#[test]
fn title_shop_me_and_run() {
    let (mut s, keys) = title_sim();
    let save = std::env::temp_dir().join(format!("ss_port_menus_{}.json", std::process::id()));
    let _ = std::fs::remove_file(&save);
    s.game.flow.save_path = Some(save.clone());
    s.game.flow.user.coins = 2000;
    assert_eq!(s.game.flow.menu, Menu::Title);
    steps(&mut s, 30);
    assert_eq!(s.game.state, GameState::Idle, "the title idles until PLAY");

    // boost shop: open the magnet card (upgrades follow the 4 single-use cards) and buy a tier
    flow::click(&mut s.game, Click::TitleShop);
    assert!(matches!(s.game.flow.menu, Menu::Shop { .. }));
    let magnet = menu::CONSUMABLES.len() + 2;
    flow::click(&mut s.game, Click::BoostCard(magnet));
    flow::click(&mut s.game, Click::BoostBuy(magnet));
    assert_eq!(s.game.flow.user.upgrades.magnet_tier, 1);
    assert_eq!(s.game.flow.user.coins, 1500);
    flow::click(&mut s.game, Click::BoostBuy(magnet));
    assert_eq!((s.game.flow.user.upgrades.magnet_tier, s.game.flow.user.coins), (2, 0), "tier 2 costs exactly the 1500 left");
    flow::click(&mut s.game, Click::BoostBuy(magnet));
    assert_eq!(s.game.flow.user.upgrades.magnet_tier, 2, "3000 for tier 3: a failed buy changes nothing");
    flow::click(&mut s.game, Click::Back);
    assert_eq!(s.game.flow.menu, Menu::Title);

    // Me panel: the original roster only (the catalog's `v_`), then Tricky
    // (3 coins) and his first outfit (2 keys)
    s.game.flow.user.coins = 3;
    flow::click(&mut s.game, Click::TitleMe);
    flow::click(&mut s.game, Click::MeTab(MeTab::Characters));
    let list = menu::characters();
    assert_eq!(list, ss_port::shop::Catalog::get().roster, "the original characters");
    assert!(list.iter().all(|c| ss_port::shop::Catalog::get().character(c).is_some()), "{list:?}");
    let tricky = list.iter().position(|c| c == "tricky").unwrap();
    flow::click(&mut s.game, Click::MeThumb(tricky));
    flow::click(&mut s.game, Click::MeFeature(1));
    assert!(matches!(menu::select_button(&s.game), Some((_, menu::SelectButton::Locked))), "outfit locked until Tricky is owned");
    flow::click(&mut s.game, Click::MeFeature(0));
    flow::click(&mut s.game, Click::MeSelect);
    assert_eq!((s.game.flow.user.selected_character.as_str(), s.game.flow.user.coins), ("tricky", 0), "buying equips");
    flow::click(&mut s.game, Click::MeFeature(1));
    flow::click(&mut s.game, Click::MeSelect);
    assert_eq!((s.game.flow.user.selected_outfit, s.game.flow.user.keys), (1, 3));
    // not enough coins for Lucy: the Ny popup, closed by any tap
    let lucy = list.iter().position(|c| c == "lucy").unwrap();
    flow::click(&mut s.game, Click::MeThumb(lucy));
    flow::click(&mut s.game, Click::MeSelect);
    assert!(matches!(s.game.flow.overlay, Some(menu::Overlay::NotEnough { ref currency, needed: 7000 }) if currency == "coins"));
    flow::click(&mut s.game, Click::Elsewhere);
    assert!(s.game.flow.overlay.is_none());
    // boards: hoverboard is selected; the Boards tab keeps it
    flow::click(&mut s.game, Click::MeTab(MeTab::Boards));
    assert!(matches!(menu::select_button(&s.game), Some((_, menu::SelectButton::Selected))));
    flow::click(&mut s.game, Click::Back);

    // the save holds it all
    let u = UserData::load(&save);
    assert_eq!(u.selected_character, "tricky");
    assert_eq!(u.selected_outfit, 1);
    assert_eq!(u.upgrades.magnet_tier, 2);
    assert!(u.owned_characters.contains(&"tricky".to_string()));
    assert!(u.owned_outfits.contains(&"tricky-outfit-1".to_string()));

    // PLAY (Space on the title)
    keys.0.borrow_mut().push(Key::Action);
    steps(&mut s, 1);
    assert!(s.game.flow.start_requested);
    s.game.flow.start_requested = false;
    s.start_run();
    steps(&mut s, 200);
    assert_eq!(s.game.state, GameState::Running);
    // the magnet upgrade: 10 + 5 * 2 seconds
    ss_port::powerups::magnet_turn_on(&mut s.game);
    assert_eq!(s.game.hero.magnet.duration, 20.0);
    steps(&mut s, 1199);
    assert!(s.game.hero.magnet.is_on());
    steps(&mut s, 2);
    assert!(!s.game.hero.magnet.is_on(), "off after 20 s");

    // no boards left: Space pauses into "Need hoverboards?"
    s.game.flow.user.hoverboards = 0;
    s.game.flow.user.coins = 300;
    keys.0.borrow_mut().push(Key::Action);
    steps(&mut s, 1);
    assert_eq!(s.game.state, GameState::Paused);
    assert!(matches!(s.game.flow.screen, Screen::BuyBoards { in_run: true, .. }));
    let z = s.game.hero.body.cz();
    steps(&mut s, 30);
    assert_eq!(s.game.hero.body.cz(), z, "paused");
    flow::click(&mut s.game, Click::BuyBoard);
    assert_eq!((s.game.flow.user.hoverboards, s.game.flow.user.coins), (1, 0));
    flow::click(&mut s.game, Click::CloseOverlay);
    assert!(matches!(s.game.flow.screen, Screen::Countdown { .. }));
    steps(&mut s, 185);
    assert_eq!(s.game.state, GameState::Running, "resumed after the 3 s countdown");
    keys.0.borrow_mut().push(Key::Action);
    steps(&mut s, 2);
    assert!(s.game.hero.hoverboard.is_on(), "the bought board");
    assert_eq!(s.game.flow.user.hoverboards, 0);
    let _ = std::fs::remove_file(&save);
}
