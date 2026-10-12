//! Native Sudoku example: solution invariants and failed-search rollback.
use nacre::{compile_to_c, native};

const EXAMPLE: &str = include_str!("../examples/sudoku.ncr");
const PUZZLE: &str = "530070000600195000098000060800060003400803001700020006060000280000419005000080079";

fn run(main: &str) -> String {
    let (class, _) = EXAMPLE.split_once("void main()").unwrap();
    let source = format!("{class}void main() {{ {main} }}");
    let c = compile_to_c(&source).unwrap_or_else(|e| panic!("{}", e.render("sudoku.ncr", &source)));
    let program = native::compile_c(&c).unwrap();
    let result = program.output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    String::from_utf8(result.stdout).unwrap()
}

fn grids(output: &str) -> Vec<Vec<u8>> {
    output.lines().filter(|line| line.starts_with('|')).map(|line| {
        line.bytes().filter_map(|b| match b {
            b'.' => Some(0),
            b'1'..=b'9' => Some(b - b'0'),
            _ => None,
        }).collect()
    }).collect::<Vec<Vec<u8>>>().chunks(9).map(|rows| rows.concat()).collect()
}

#[test]
fn solves_preserving_clues_and_all_units() {
    let output = run(&format!("Sudoku s = new Sudoku(\"{PUZZLE}\"); println(s.solve()); println(s.isComplete()); s.show();"));
    assert_eq!(output.lines().next(), Some("true"));
    assert_eq!(output.lines().nth(1), Some("true"));
    let boards = grids(&output);
    assert_eq!(boards.len(), 1);
    let board = &boards[0];
    assert_eq!(board.len(), 81);
    for (i, clue) in PUZZLE.bytes().enumerate() {
        if clue != b'0' { assert_eq!(board[i], clue - b'0'); }
    }
    for unit in 0..9 {
        let mut row: Vec<_> = (0..9).map(|i| board[unit * 9 + i]).collect();
        let mut column: Vec<_> = (0..9).map(|i| board[i * 9 + unit]).collect();
        let mut box_cells: Vec<_> = (0..9).map(|i| board[(unit / 3 * 3 + i / 3) * 9 + unit % 3 * 3 + i % 3]).collect();
        for values in [&mut row, &mut column, &mut box_cells] {
            values.sort_unstable();
            assert_eq!(values.as_slice(), &[1, 2, 3, 4, 5, 6, 7, 8, 9]);
        }
    }
}

#[test]
fn recognizes_any_valid_complete_grid() {
    let solution = "123456789456789123789123456234567891567891234891234567345678912678912345912345678";
    assert_eq!(run(&format!("Sudoku s = new Sudoku(\"{solution}\"); println(s.isComplete());")), "true\n");
}

#[test]
fn unsolvable_search_restores_every_cell() {
    // A locally legal 1 at row 1, column 3 makes this puzzle unsolvable.
    let puzzle = PUZZLE.replacen("530", "531", 1);
    let output = run(&format!("Sudoku s = new Sudoku(\"{puzzle}\"); println(s.isValid()); s.show(); println(s.solve()); s.show();"));
    assert_eq!(output.lines().next(), Some("true"));
    assert!(output.lines().any(|line| line == "false"));
    let boards = grids(&output);
    let expected: Vec<_> = puzzle.bytes().map(|b| b - b'0').collect();
    assert_eq!(boards, vec![expected.clone(), expected]);
}

#[test]
fn rejects_bad_shape_digits_and_each_conflict_kind() {
    let mut puzzles = vec![String::new(), "0".repeat(80), "0".repeat(82), format!("x{}", "0".repeat(80))];
    for other in [1, 9, 10] { // Same row, same column, box only.
        let mut cells = vec![b'0'; 81];
        cells[0] = b'5';
        cells[other] = b'5';
        puzzles.push(String::from_utf8(cells).unwrap());
    }
    let mut main = String::new();
    for (i, puzzle) in puzzles.iter().enumerate() {
        main.push_str(&format!("Sudoku s{i} = new Sudoku(\"{puzzle}\"); println(s{i}.isValid()); println(s{i}.solve());"));
    }
    assert_eq!(run(&main), "false\n".repeat(puzzles.len() * 2));
}
