//! Daily Word Hunt (deobfuscated.js 10445-10688; docs/js_notes/word_hunt.md).
//!
//! The word of the day is `va[(days since 2021-06-28) % 21]` (`xa`). The
//! letters spawn one at a time (`Na()` = the next letter, pickup timer 20 s in
//! `wg`); collecting one (`Fa`) advances the progress, saved per day (the
//! original's `currentLetter` is today's date stamp with the index spliced in,
//! so a new day starts at 0). The last letter pays `ya[n]` (`n` = hunts done
//! this week before today): mystery box, mystery box, 1050 coins, 2100 coins,
//! super mystery box; coins go straight to the user, a box is rolled into the
//! run's prizes. Finished words are listed (`completedHunts`) until the word
//! changes on a Monday.
//!
//! The port saves the same facts readably: `hunt_word`, `hunt_day` +
//! `hunt_index` (the index counts only on that day), `completed_hunts`,
//! `word_hunt_started`. Days are UTC days (the original mixes UTC dates with
//! local midnights).

use crate::game::Game;

/// `va`.
pub const WORDS: [&str; 21] = [
    "jake", "tricky", "fresh", "spike", "yutani", "kiloo", "sybo", "poki", "coins", "keys", "mission", "hats", "guitar", "stereo", "surfer",
    "subway", "magnet", "pogo", "ufos", "run", "bye",
];

/// `ya`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reward {
    Box(&'static str),
    Coins(i64),
}

pub const REWARDS: [Reward; 5] =
    [Reward::Box("mystery-box"), Reward::Box("mystery-box"), Reward::Coins(1050), Reward::Coins(2100), Reward::Box("super-mystery-box")];

/// `new Date("06/28/2021")` as a day number (days since 1970-01-01).
const EPOCH_DAY: i64 = 18806;

/// A calendar day: days since 1970-01-01 (UTC).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Day(pub i64);

impl Day {
    pub fn today() -> Self {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        Day(secs.div_euclid(86400))
    }
    /// `Ta()`: 0 = Monday .. 6 = Sunday (1970-01-01 was a Thursday).
    pub fn weekday(self) -> i64 {
        (self.0 + 3).rem_euclid(7)
    }
    /// `xa`.
    pub fn word(self) -> &'static str {
        WORDS[(self.0 - EPOCH_DAY).rem_euclid(WORDS.len() as i64) as usize]
    }
}

/// The run-time state (`Sa`, `Ca`, `wa`).
#[derive(Clone, Debug, PartialEq)]
pub struct WordHunt {
    pub day: Day,
    pub word: &'static str,
    /// `Sa`: letters collected today.
    pub index: usize,
    /// `wa`: today's word is done.
    pub done: bool,
    /// `Ca`: hunts finished this week before today's.
    pub completed_before: usize,
}

impl WordHunt {
    /// `Na()`: the letter to spawn (upper case, as the model groups).
    pub fn next_letter(&self) -> Option<String> {
        self.word.chars().nth(self.index).map(|c| c.to_ascii_uppercase().to_string())
    }
    /// `Oa()`.
    pub fn reward(&self) -> Option<Reward> {
        REWARDS.get(self.completed_before).copied()
    }
}

/// `Da()` (with `Ea()`), at start-up: today's word, the saved progress, the
/// week's finished hunts.
pub fn init(user: &mut crate::flow::UserData, day: Day) -> WordHunt {
    let word = day.word();
    user.completed_hunts.retain(|w| WORDS.contains(&w.as_str()));
    if user.hunt_word != word {
        user.hunt_word = word.to_string();
        user.hunt_index = 0;
        user.hunt_day = 0;
        user.word_hunt_started = false;
        if day.weekday() == 0 {
            user.completed_hunts.clear();
        }
    }
    let index = if user.hunt_day == day.0 { user.hunt_index.min(word.len()) } else { 0 };
    if index == word.len() && !user.completed_hunts.iter().any(|w| w == word) {
        user.completed_hunts.push(word.to_string());
    }
    let done = user.completed_hunts.iter().any(|w| w == word);
    let completed_before = user.completed_hunts.len() - usize::from(done);
    if !done && !user.word_hunt_started {
        user.word_hunt_started = true;
    }
    WordHunt { day, word, index, done, completed_before }
}

/// Start a game with the hunt: the letter to spawn; no letter timer when
/// today's word is already done (`wg` only adds it while `!ja()`).
pub fn attach(g: &mut Game, hunt: WordHunt) {
    g.hunt_letter = hunt.next_letter();
    if hunt.done {
        g.timed_pickups.retain(|t| t.0 != "letter");
    }
    g.word_hunt = Some(hunt);
}

/// `Fa()`: a letter collected.
pub fn collect(g: &mut Game) {
    let Some(h) = g.word_hunt.as_mut() else { return };
    if h.done {
        return;
    }
    h.done = h.index + 1 == h.word.len();
    let (index, done, word, day) = (h.index, h.done, h.word, h.day);
    // ka.dispatch(Sa): the HUD banner and the pause panel's letters
    g.hud.word_banner = Some(crate::flow::WordBanner { letter: index, t: 0.0, complete_shown: false });
    if done && !g.flow.user.completed_hunts.iter().any(|w| w == word) {
        match g.word_hunt.as_ref().and_then(|h| h.reward()) {
            Some(Reward::Coins(n)) => g.flow.user.coins += n,
            Some(Reward::Box(kind)) => {
                let p = crate::prizes::roll(g, kind);
                g.prizes.push(p);
            }
            None => {}
        }
        g.flow.user.completed_hunts.push(word.to_string());
        // Aa.dispatch(): a daily challenge done; no more letter timer, the
        // letters leave the scene
        crate::missions::add_stat(g, 1, "mission-daily-challenges");
        g.timed_pickups.retain(|t| t.0 != "letter");
        let letters: Vec<_> = g
            .level_entities
            .iter()
            .copied()
            .filter(|&id| g.ents[id.0].cls == crate::entities::Cls::Pickup(crate::entities::PickupKind::Letter) && g.ents[id.0].in_scene)
            .collect();
        for id in letters {
            g.remove_child(id);
        }
    }
    let h = g.word_hunt.as_mut().unwrap();
    h.index += 1;
    g.hunt_letter = h.next_letter();
    g.flow.user.hunt_day = day.0;
    g.flow.user.hunt_index = index + 1;
    g.flow.save_user();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::UserData;

    #[test]
    fn day_word_and_weekday() {
        // 2021-06-28 was a Monday: the list starts there
        let d = Day(EPOCH_DAY);
        assert_eq!(d.weekday(), 0);
        assert_eq!(d.word(), "jake");
        assert_eq!(Day(EPOCH_DAY + 14).word(), "surfer");
        assert_eq!(Day(EPOCH_DAY + 21).word(), "jake");
    }

    #[test]
    fn progress_counts_only_today_and_week_resets_on_monday() {
        let mut u = UserData::default();
        let monday = Day(EPOCH_DAY + 7 * 200); // a Monday
        assert_eq!(monday.weekday(), 0);
        let h = init(&mut u, monday);
        assert_eq!((h.index, h.done, h.completed_before), (0, false, 0));
        assert_eq!(h.next_letter().as_deref(), Some(&h.word[..1].to_ascii_uppercase()[..]));
        u.hunt_day = monday.0;
        u.hunt_index = 2;
        assert_eq!(init(&mut u, monday).index, 2, "same day keeps the progress");
        // the next day: a new word, progress 0, the finished list kept (not Monday)
        u.completed_hunts = vec![monday.word().to_string()];
        let tue = Day(monday.0 + 1);
        let h = init(&mut u, tue);
        assert_eq!((h.index, h.done, h.completed_before), (0, false, 1));
        assert_eq!(h.reward(), Some(REWARDS[1]));
        // a week later the Monday's new word clears the list
        let next_monday = Day(monday.0 + 7);
        init(&mut u, next_monday);
        assert!(u.completed_hunts.is_empty());
    }
}
