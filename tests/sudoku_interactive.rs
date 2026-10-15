//! Native tests for the interactive Sudoku game and its save format.
use nacre::{compile_to_c, native};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const EXAMPLE: &str = include_str!("../examples/apps/sudoku.ncr");
const PUZZLE: &str = "530070000600195000098000060800060003400803001700020006060000280000419005000080079";
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let serial = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("nacre-sudoku-{}-{stamp}-{serial}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn program() -> native::NativeProgram {
    let c = compile_to_c(EXAMPLE).unwrap_or_else(|e| panic!("{}", e.render("apps/sudoku.ncr", EXAMPLE)));
    native::compile_c(&c).unwrap()
}

fn run(program: &native::NativeProgram, input: &str, args: &[PathBuf]) -> Output {
    let mut child = Command::new(program.path())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).unwrap();
    drop(stdin);
    child.wait_with_output().unwrap()
}

fn successful(output: Output) -> String {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

fn boards(output: &str) -> Vec<Vec<u8>> {
    output.lines().filter(|line| line.starts_with('|')).map(|line| {
        line.bytes().filter_map(|byte| match byte {
            b'.' => Some(0),
            b'1'..=b'9' => Some(byte - b'0'),
            _ => None,
        }).collect()
    }).collect::<Vec<Vec<u8>>>().chunks(9).map(|rows| rows.concat()).collect()
}

fn puzzle_board() -> Vec<u8> {
    PUZZLE.bytes().map(|byte| byte - b'0').collect()
}


#[test]
fn eof_ends_the_interactive_session() {
    let program = program();
    let output = run(&program, "", &[]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}
#[test]
fn gameplay_recovers_from_bad_moves_and_protects_clues() {
    let program = program();
    let output = successful(run(
        &program,
        "set 1 nope 3\nset 1 3 4\nset 1 3 2\nset 1 3 5\nset 1 3 8\nset 1 1 7\nclear 1 1\nshow\nclear 1 3\nshow\nquit\n",
        &[],
    ));
    let boards = boards(&output);
    let original = puzzle_board();
    let mut after_first_move = original.clone();
    after_first_move[2] = 4;
    let mut after_replacement = original.clone();
    after_replacement[2] = 2;
    let first_move = boards.iter().position(|board| board == &after_first_move).unwrap();
    let replacement = boards.iter().position(|board| board == &after_replacement).unwrap();
    assert_eq!(boards.first(), Some(&original));
    assert!(first_move < replacement);
    assert!(boards.iter().all(|board| board == &original || board == &after_first_move || board == &after_replacement));
    assert_eq!(boards.last(), Some(&original));
    assert_eq!(boards.last().unwrap()[0], 5); // The clue is still present after a rejected clear.
}

#[test]
fn saved_moves_round_trip_and_can_be_the_initial_file() {
    let program = program();
    let scratch = Scratch::new();
    let save = scratch.file("partie avec espaces.sud");
    let save_path = save.to_string_lossy();
    let commands = format!("set 1 3 4\nsave {save_path}\nclear 1 3\nload {save_path}\nquit\n");
    let output = successful(run(&program, &commands, &[]));
    let mut moves = vec![b'0'; 81];
    moves[2] = b'4';
    let moves = String::from_utf8(moves).unwrap();
    let expected_file = format!("SUDOKU1\n{PUZZLE}\n{moves}\n");
    assert_eq!(fs::read_to_string(&save).unwrap(), expected_file);

    let mut expected_board = puzzle_board();
    expected_board[2] = 4;
    assert_eq!(boards(&output).last(), Some(&expected_board));

    let initial_file_output = successful(run(&program, "clear 1 3\nshow\nquit\n", std::slice::from_ref(&save)));
    let initial_boards = boards(&initial_file_output);
    assert_eq!(initial_boards.first(), Some(&expected_board));
    let reset_board = puzzle_board();
    assert_eq!(initial_boards.last(), Some(&reset_board));
}


#[test]
fn invalid_load_is_rejected_without_changing_the_current_game() {
    let program = program();
    let scratch = Scratch::new();
    let invalid = scratch.file("invalid.sud");
    let invalid_path = invalid.to_string_lossy();
    let mut modified_clue_moves = vec![b'0'; 81];
    modified_clue_moves[0] = b'1'; // A move is forbidden where the clue grid already has a clue.
    modified_clue_moves[2] = b'4';
    let clue_change = format!("SUDOKU1\n{PUZZLE}\n{}\n", String::from_utf8(modified_clue_moves).unwrap());
    fs::write(&invalid, clue_change).unwrap();

    let conflict = scratch.file("conflict.sud");
    let conflict_path = conflict.to_string_lossy();
    let mut conflicting_moves = vec![b'0'; 81];
    conflicting_moves[2] = b'5'; // This move conflicts with the row's fixed 5.
    let conflict_state = format!("SUDOKU1\n{PUZZLE}\n{}\n", String::from_utf8(conflicting_moves).unwrap());
    fs::write(&conflict, conflict_state).unwrap();

    let malformed = scratch.file("malformed.sud");
    let malformed_path = malformed.to_string_lossy();
    fs::write(&malformed, "not a Sudoku save\n").unwrap();

    let commands = format!(
        "set 1 3 4\nload {invalid_path}\nshow\nload {conflict_path}\nshow\nload {malformed_path}\nshow\nset 1 3 2\nquit\n",
    );
    let output = successful(run(&program, &commands, &[]));
    let original = puzzle_board();
    let mut preserved_board = original.clone();
    preserved_board[2] = 4;
    let mut next_move = original;
    next_move[2] = 2;
    let displayed = boards(&output);
    let preserved = displayed.iter().position(|board| board == &preserved_board).unwrap();
    let continued = displayed.iter().rposition(|board| board == &next_move).unwrap();
    assert!(preserved < continued);
    assert!(displayed[preserved..continued].iter().all(|board| board == &preserved_board));
    assert_eq!(displayed.last(), Some(&next_move));
}
