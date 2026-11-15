//! However the viewer was driven before, spec mode shows the same tree: a
//! seeded generator builds varied `.kiro` projects and random key histories
//! (mode switches mixed with cursor and fold moves), and the spec-mode tree
//! panel must match a viewer that never switched. A failure prints its seed.

use super::round_trip_tests::Session;
use super::tests::scratch_dir;
use crossterm::event::KeyCode;
use std::fs;
use std::path::Path;

const SEED_COUNT: u64 = 40;
const MAX_HISTORY_LEN: u64 = 40;

/// xorshift64*: small and deterministic, so a seed alone reproduces a case
/// without adding a dependency.
struct SeededRng(u64);

impl SeededRng {
    fn new(seed: u64) -> Self {
        // A zero state would stay zero forever.
        SeededRng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }

    fn chance(&mut self, one_in: u64) -> bool {
        self.below(one_in) == 0
    }
}

const CORE_DOCS: [&str; 4] = ["requirements.md", "biz-process.md", "design.md", "tasks.md"];

fn write_spec(kiro: &Path, name: &str, rng: &mut SeededRng) {
    let dir = kiro.join("specs").join(name);
    fs::create_dir_all(&dir).expect("spec dir");
    fs::write(
        dir.join("spec.json"),
        format!("{{\"name\":\"{name}\",\"phase\":\"design\",\"approvals\":{{}}}}"),
    )
    .expect("spec.json");
    for doc in CORE_DOCS {
        if rng.chance(2) {
            fs::write(dir.join(doc), format!("# {name} {doc}\n")).expect("core doc");
        }
    }
    for extra in 0..rng.below(2) {
        fs::write(dir.join(format!("extra-{extra}.md")), "# Extra\n").expect("extra doc");
    }
    let mut nested = dir.clone();
    for depth in 0..rng.below(3) {
        nested = nested.join(format!("level-{depth}"));
        fs::create_dir_all(&nested).expect("nested dir");
        fs::write(nested.join("inner.md"), "# Inner\n").expect("nested doc");
    }
}

fn write_steering(kiro: &Path, rng: &mut SeededRng) {
    let mut dir = kiro.join("steering");
    fs::create_dir_all(&dir).expect("steering dir");
    for depth in 0..=rng.below(3) {
        fs::write(dir.join(format!("steer-{depth}.md")), "# Steer\n").expect("steering doc");
        dir = dir.join(format!("group-{depth}"));
        fs::create_dir_all(&dir).expect("steering subdir");
    }
}

/// A random `.kiro` whose depth, spec count and document count all vary with
/// the seed; sizes stay small enough that a whole expanded tree fits a frame.
fn generate_project(seed: u64) -> std::path::PathBuf {
    let mut rng = SeededRng::new(seed);
    let project = scratch_dir(&format!("history_invariance_{seed}"));
    let kiro = project.join(".kiro");
    for index in 0..=rng.below(3) {
        write_spec(&kiro, &format!("spec-{index}"), &mut rng);
    }
    if rng.chance(2) {
        write_steering(&kiro, &mut rng);
    }
    project
}

fn drive_random_history(session: &mut Session, rng: &mut SeededRng) -> Vec<&'static str> {
    let mut history = Vec::new();
    for _ in 0..rng.below(MAX_HISTORY_LEN) {
        let (label, code) = match rng.below(8) {
            0..=2 => ("Space", KeyCode::Char(' ')),
            3 => ("Down", KeyCode::Down),
            4 => ("Up", KeyCode::Up),
            5 => ("Right", KeyCode::Right),
            6 => ("o", KeyCode::Char('o')),
            _ => ("c", KeyCode::Char('c')),
        };
        history.push(label);
        session.press(code);
    }
    if session.is_files_mode() {
        history.push("Space");
        session.press(KeyCode::Char(' '));
    }
    history
}

fn expanded_spec_tree_rows(session: &mut Session) -> Vec<String> {
    session.expand_everything();
    session.tree_rows()
}

#[test]
fn spec_mode_tree_is_identical_whatever_mode_switches_and_moves_came_before() {
    for seed in 0..SEED_COUNT {
        let project = generate_project(seed);
        let baseline = expanded_spec_tree_rows(&mut Session::open_spec_mode(&project));

        let mut session = Session::open_spec_mode(&project);
        let history = drive_random_history(&mut session, &mut SeededRng::new(seed ^ 0xA5A5));
        assert!(!session.is_files_mode(), "seed {seed}: history must end in spec mode");
        let after_history = expanded_spec_tree_rows(&mut session);

        assert_eq!(
            after_history, baseline,
            "seed {seed}: spec tree changed after history {history:?}"
        );
        fs::remove_dir_all(&project).ok();
    }
}
