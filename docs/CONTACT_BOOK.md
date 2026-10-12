# Contact book

`examples/contact_book.ncr` is a complete interactive contact manager. Each
contact owns a `Text` name and an optional `Text` phone number. Display uses
shared references into the list; edits use an exclusive element reference and
replace the owned fields in place.

## Run

From the repository root, start it with:

```sh
cargo run --offline --locked -- run examples/contact_book.ncr
```

The program starts with an empty in-memory book. Type a command on its own line:

| Command | Prompts and behavior |
|---|---|
| `list` | Display all contacts and their one-based positions. |
| `show` | Prompt for a one-based contact position and display that contact. |
| `add` | Prompt for a name and phone number; an empty phone line means no phone. |
| `edit` | Prompt for a position, replacement name, and replacement phone; an empty phone line clears the phone number. |
| `remove` | Prompt for a position and remove that contact. |
| `search` | Prompt for a case-sensitive name substring and display matching contacts at their original positions. An empty query matches all names. |
| `filter` | Prompt for `with` or `without` and show contacts with or without a phone number. |
| `remove-matching` | Preview a name-substring selection; remove it only after exact `yes` confirmation. Survivors retain their order. |
| `load` | Prompt for a file path and load a saved book. |
| `save` | Prompt for a file path and atomically save the book. |
| `help` | Print the command summary. |
| `quit` | Leave the program. |

Prompts accept the next complete input line. In particular, enter paths without
quotes: spaces and Unicode characters are accepted. Invalid positions, malformed
save data, and ordinary file errors are reported without discarding the current
book. End-of-file or a console read error exits the interactive loop.

Search compares valid UTF-8 byte sequences, without slicing or allocating
temporary names. It does not normalize text or fold case. Preview and filtering
use shared list traversal. Batch removal ends those loans before editing the
list, walks selected positions in reverse, and uses `tryRemove`; cancellation,
invalid confirmation, and EOF at confirmation do not change the book. The
single-contact remove command also recovers from invalid positions.

## Persistence format

Saved files use version 1. Metadata is non-negative decimal ASCII terminated by
LF (`\n`); field lengths count UTF-8 **bytes**, not characters. The file contains:

1. Version (`1`) and contact count, each on its own metadata line.
2. For each contact, the name byte length, the exact name bytes, and an LF.
3. A phone-present line (`0` or `1`) and an LF. If present, the phone byte
   length, exact phone bytes, and an LF.

For example, a one-contact book containing the name `Ada` and phone `+123` is:

```text
1
1
3
Ada
1
4
+123
```

A name or phone may itself contain LF bytes: the preceding byte length determines
where the field ends. The LF after the exact field bytes is the format separator.
The loader checks the version, decimal syntax and overflow, field bounds and UTF-8
boundaries, required separators, and absence of trailing data. It builds and
validates a complete candidate list before replacing the current book, so a
failed load leaves existing contacts unchanged. File saves use
`Files.writeTextAtomic` to replace the destination atomically.

`examples/invalid/contact_delete_during_borrow.ncr` is the rejected counterpart:
it keeps a shared element reference alive while trying to remove that element
from its list.
