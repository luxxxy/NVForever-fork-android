//! The companion perks that mark what is around while the player aims
//! (docs/spec-notes/perk-entry-points.md §13):
//!
//! - **Spotter** (Boone; entry point 70 Has Improved Spotting, "Enemy highlighting"
//!   [documented: GECK wiki Perk entry points]): while the player aims, the actors hostile
//!   to the player (`crate::compass`'s rule: fighting the player or ready to attack it,
//!   aware of the player or not) as far as actors are drawn ([`ActorDrawDistance`]: the
//!   loaded cells, cut to the Actors draw-distance slider when it is below 100%) play the
//!   DOBJ's Spotter Effect, `fxSpotterShader` 0015FF60 [observed: FalloutNV.esm's DOBJ
//!   slot 31]. The original marks "every enemy in render distance" [observed 2026-10-31,
//!   owner, observer check 19; #1171], farther than the detection reach (5,000 units
//!   outdoors) used before; and farther than the compass's (Perception × 300), which left
//!   the enemies the player spots before they notice it unmarked [owner's playtest, #1094].
//! - **Search and Mark** (Rex; entry point 71 Has Improved Item Detection, "Item
//!   highlighting" [documented: the same page]): while the player aims, the chems, guns and
//!   ammunition lying within [`SEARCH_RANGE`] play the DOBJ's Item Detected Effect,
//!   `fxSearchAndMark` 0015FF61 [observed: slot 32]. Which items: ALCH of equip type Chems
//!   or Stimpak, WEAP of skill Guns or Energy Weapons, AMMO [hypothesis: the fandom wiki's
//!   "chems, firearms & ammunition"; the records say nothing of it].
//!
//! The marks are what [`PerkMarks`] holds; the body draws them as effect shaders
//! (nvforever `effect_shaders`), until the player stops aiming or the mark leaves.
//! Worked out ten times a second. Log: `perk marks: <perk> marks <n> <what> while the
//! player aims` when the count changes.

use crate::combat::Combat;
use crate::control::PlayerControl;
use crate::phys::PhysState;
use crate::player::Player;
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use crate::{PlaySet, PlayState};
use bevy::prelude::*;
use fnv_game::data::DefaultObject;
use fnv_sim::FormId;
use std::collections::{BTreeSet, HashMap};

/// How near an item must lie for Search and Mark, game units [design: the fandom wiki's
/// "within a short distance"; no setting names it].
pub const SEARCH_RANGE: f32 = 1500.0;

/// How far actors are drawn from the camera, game units; `None`: as far as their cells
/// are loaded (the Actors draw-distance slider at 100%). The body keeps it (nvforever
/// `draw_distance.rs`); Spotter marks no farther (module notes).
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct ActorDrawDistance(pub Option<f32>);

/// Whether an actor `dist` units away is drawn, and so can be marked by Spotter: every
/// actor of the loaded cells ([`crate::compass`]'s `actors_near`) when the draw distance
/// is the cells', otherwise those within it [design: measured from the player's feet,
/// not the camera; the two are a few units apart].
pub fn spotter_reaches(dist: f32, drawn: ActorDrawDistance) -> bool {
    drawn.0.is_none_or(|d| dist <= d)
}

/// Seconds between updates [design].
const POLL: f32 = 0.1;

/// What the perks mark now: (reference, effect shader, the perk's name for the log).
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct PerkMarks(pub Vec<(FormId, FormId, &'static str)>);

/// Scratch storage retained between perk-mark polls; these scans run only while aiming,
/// but their temporary collections used to allocate afresh on every poll.
#[derive(Default)]
struct MarkScratch {
    bodies: HashMap<u32, [f32; 3]>,
    item_ids: BTreeSet<FormId>,
    now: Vec<(FormId, FormId, &'static str)>,
}

pub struct PerkMarksPlugin;

impl Plugin for PerkMarksPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PerkMarks>().init_resource::<ActorDrawDistance>().add_systems(Update, mark.in_set(PlaySet::Ui));
    }
}

/// Whether an item base is one Search and Mark marks.
pub fn searched(data: &fnv_game::data::GameData, base: FormId) -> bool {
    use fnv_game::actor_values::{ENERGY_WEAPONS, GUNS};
    use fnv_game::data::IngestibleDef;
    match data.sig(base).as_ref() {
        Some(b"ALCH") => data.ingestibles.get(&base).is_some_and(|i| i.equip_type == IngestibleDef::CHEMS || i.equip_type == IngestibleDef::STIMPAK),
        Some(b"WEAP") => data.weapon_skills.get(&base).is_some_and(|s| *s == GUNS as i64 || *s == ENERGY_WEAPONS as i64),
        Some(b"AMMO") => true,
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn mark(
    time: Res<Time>,
    state: Res<State<PlayState>>,
    control: Res<PlayerControl>,
    player: Option<Res<Player>>,
    phys: Option<Res<PhysState>>,
    combat: Option<Res<Combat>>,
    drawn: Res<ActorDrawDistance>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut marks: ResMut<PerkMarks>,
    (mut poll, mut logged, mut scratch): (Local<f32>, Local<[usize; 2]>, Local<MarkScratch>),
) {
    *poll += time.delta_secs();
    if *poll < POLL {
        return;
    }
    *poll = 0.0;
    let aiming = *state.get() == PlayState::Playing && control.aiming && control.weapon_out && combat.is_some();
    scratch.now.clear();
    if let (true, Some(player), Some(phys), Some(rt)) = (aiming, player, phys, script.as_deref_mut().and_then(|s| s.rt.as_mut())) {
        let feet = player.feet;
        let drawn_distance_sq = drawn.0.map(|distance| distance * distance);
        scratch.bodies.clear();
        scratch.bodies.extend(phys.npcs.iter().map(|(&id, body)| (id, body.ch.feet)));
        let (bodies, item_ids, now) = (&scratch.bodies, &mut scratch.item_ids, &mut scratch.now);
        with_world(rt, &mut sim, |rt| {
            let g = &mut rt.game;
            let distance_sq = |at: [f32; 3]| {
                let x = at[0] - feet[0];
                let y = at[1] - feet[1];
                let z = at[2] - feet[2];
                x * x + y * y + z * z
            };
            if let Some(shader) = g.data.default_object(DefaultObject::SpotterEffect).filter(|_| g.player_has_entry(fnv_game::perks::HAS_IMPROVED_SPOTTING)) {
                for (r, at) in crate::compass::actors_near(g, bodies) {
                    if drawn_distance_sq.is_some_and(|limit| distance_sq(at) > limit) || g.is_dead(r) || !g.present(r) {
                        continue;
                    }
                    let fighting = g.awareness.get(r).and_then(|a| a.combat_target()).is_some_and(|(t, _)| t == FormId::PLAYER_REF);
                    if fighting || g.would_attack(r, FormId::PLAYER_REF) {
                        now.push((r, shader, "Spotter"));
                    }
                }
            }
            if let Some(shader) = g.data.default_object(DefaultObject::ItemDetectedEffect).filter(|_| g.player_has_entry(fnv_game::perks::HAS_IMPROVED_ITEM_DETECTION)) {
                let placed = g.world.placed;
                item_ids.clear();
                item_ids.extend(g.loaded_cells.iter().filter_map(|c| placed.by_cell.get(c)).flatten().copied());
                item_ids.extend(g.moved.iter().copied());
                for r in item_ids.iter().copied() {
                    let Some(p) = placed.get(r).filter(|p| &p.sig == b"REFR") else { continue };
                    if !searched(g.data, p.base) || !g.present(r) || !g.cell_of(r).is_some_and(|c| g.loaded_cells.contains(&c)) {
                        continue;
                    }
                    let at = g.world.peek(r).map_or(p.pos, |s| s.pos);
                    if distance_sq(at) <= SEARCH_RANGE * SEARCH_RANGE {
                        now.push((r, shader, "Search and Mark"));
                    }
                }
                // Items dropped or placed at run time (`World::created`), visited in place
                // rather than first collecting a temporary Vec.
                for (r, item) in g.world.created() {
                    if !item.deleted()
                        && item.enabled()
                        && searched(g.data, item.base)
                        && item.cell.is_some_and(|c| g.loaded_cells.contains(&c))
                        && distance_sq(item.pos) <= SEARCH_RANGE * SEARCH_RANGE
                    {
                        now.push((r, shader, "Search and Mark"));
                    }
                }
            }
        });
    }
    let counts = scratch.now.iter().fold([0, 0], |mut counts, mark| {
        counts[usize::from(mark.2 != "Spotter")] += 1;
        counts
    });
    for (i, (perk, what)) in [("Spotter", "hostile actors"), ("Search and Mark", "items")].into_iter().enumerate() {
        if counts[i] != logged[i] {
            logged[i] = counts[i];
            info!("perk marks: {perk} marks {} {what} while the player aims", counts[i]);
        }
    }
    if marks.0 != scratch.now {
        marks.0.clone_from(&scratch.now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spotter_reaches_every_drawn_actor_past_the_detection_reach() {
        // The draw distance at its full slider: every actor of the loaded cells, however
        // far (#1171: the detection reach, 5,000 units outdoors, stopped it before).
        assert!(spotter_reaches(5_001.0, ActorDrawDistance(None)));
        assert!(spotter_reaches(40_000.0, ActorDrawDistance(None)));
        // The Actors slider cut down: no farther than actors are drawn.
        let half = ActorDrawDistance(Some(8_000.0));
        assert!(spotter_reaches(7_999.0, half));
        assert!(spotter_reaches(8_000.0, half));
        assert!(!spotter_reaches(8_001.0, half));
    }
}
