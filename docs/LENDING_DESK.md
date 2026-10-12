# Lending desk

`examples/lending_desk/` is a multi-file Nacre source bundle. It starts with an
empty catalog and member registry. Books and tools implement the same `Loanable`
interface and share owned label/availability data. The generic `Catalog<T>`
combines `Map<int,T>` with a separate insertion-ordered ID list; the application
uses `Catalog<Loanable>` and `Catalog<Member>` so resource display and persistence
remain polymorphic without map iteration.

## Run

From the repository root:

```sh
cargo run --offline --locked -- run examples/lending_desk
```

An optional initial save file can be supplied through program arguments:

```sh
cargo run --offline --locked -- run examples/lending_desk -- "path with spaces/book Ω.data"
```

The desk does not use a built-in data file or workstation path. Enter paths at
the `File path:` prompt without quotes; spaces and Unicode are accepted. The
logical day starts at zero. Resource and member IDs are positive, stable, and
never reused. `advance` changes the explicit logical day; due dates are computed
from that day and each resource's lending period, without a wall-clock API.

## Commands and prompts

Each command is entered at `lending-desk> `. Prompts below are issued in order;
a command does not mutate the desk until all of its required input has arrived
and passed validation.

| Command | Prompts | Behavior |
|---|---|---|
| `add-book` | `Book title: `, `Author: `, `Lending period in days: ` | Add an owned book with a positive lending period. |
| `add-tool` | `Tool name: `, `Tool category/details: `, `Lending period in days: ` | Add an owned tool with a positive lending period. |
| `add-member` | `Member name: `, `Note (leave blank for none): ` | Add a member; an empty note means `None`, otherwise it is saved as owned text. |
| `list` | — | Display resources in insertion order, including kind, label, details, lending period, availability, and the active borrower's name. |
| `members` | — | Display members in insertion order, including optional notes. |
| `show` | `Resource ID: ` | Display one resource by stable ID. |
| `lend` | `Resource ID: `, `Member ID: ` | Lend an available resource to an existing member. Its due day is current day plus lending period; overflow is rejected before mutation. |
| `return` | `Resource ID: ` | Return a currently lent resource to `Available`. |
| `maintenance` | `Resource ID: `, `Maintenance reason (blank releases to available): ` | Put a non-lent resource in maintenance with a non-empty reason. An empty reason makes it available. Lent resources cannot enter or leave maintenance. |
| `rename` | `Resource ID: `, `Rename mode (replace/append): `, `Label text: ` | Replace the owned label, or append text through an exclusive borrowed label reference. |
| `remove` | `Resource ID: ` | Remove an existing non-lent resource. |
| `remove-member` | `Member ID: ` | Remove an existing member only when no active loan refers to that member. |
| `advance` | `Days to advance: ` | Add a non-negative amount to the logical day, rejecting overflow before mutation. |
| `load` | `File path: ` | Parse and cross-validate a complete candidate desk before replacing active state. Read or validation errors preserve the current desk. |
| `save` | `File path: ` | Atomically replace the requested file with a complete snapshot. |
| `help` | — | Print the command and domain-rule summary. |
| `quit` | — | Leave the interactive loop. |

Resource and member ID prompts require positive signed-32-bit values. Day and
period prompts use non-negative/positive signed-32-bit values respectively.
Invalid numbers, unknown IDs, unavailable resources, illegal relationship
changes, and arithmetic overflow report an error and leave the command's state
unchanged. EOF or a console read error exits the loop; EOF during a multi-prompt
command does not commit the incomplete command.

## Persistence format

Files use version 1. All metadata is non-negative decimal ASCII followed by one
LF byte (`\n`); IDs in records and next-ID counters must be positive. Lengths
count UTF-8 **bytes**, not Unicode characters. Each length-prefixed field is
followed by one LF separator after exactly its declared bytes. Field contents may
contain LF bytes. The reader checks that each field begins and ends on a UTF-8
boundary. Tags are one ASCII byte followed by LF. There are no blank lines or
trailing bytes.

The records appear in this order:

1. Header metadata lines: version (`1`), logical day, next resource ID, next
   member ID, resource count, and member count.
2. Each resource, in catalog insertion order: positive ID; kind tag (`B` for
   book or `T` for tool); label byte length and label field; detail byte length
   and detail field (book author or tool category/details); positive lending
   period; availability tag (`A`, `L`, or `M`).
   - `A` has no payload.
   - `L` is followed by positive member ID and non-negative due day, each on a
     metadata line.
   - `M` is followed by reason byte length and reason field; the reason must be
     non-empty.
3. Each member, in catalog insertion order: positive ID; name byte length and
   name field; note flag (`0` or `1`) on a metadata line. Flag `0` has no more
   fields. Flag `1` is followed by note byte length and note field; an empty
   note is distinct from an absent note.

For example, one available 14-day book named `Ada`, by `Le Guin`, and one member
`Mina` with no note is represented exactly as:

```text
1
0
2
2
1
1
1
B
3
Ada
7
Le Guin
14
A
1
4
Mina
0
```

The loader rejects unsupported versions, malformed or overflowing numbers,
truncated fields, invalid UTF-8 boundaries, missing separators, unknown tags,
non-positive/duplicate IDs, inconsistent next-ID counters, non-positive lending
periods, invalid availability data, missing member references, empty maintenance
reasons, and trailing data. It constructs a complete owned candidate and checks
all loan/member relationships before replacing the active desk. A failed load
preserves the prior catalogs, order, day, and ID counters. Saves use
`Files.writeTextAtomic`: replacement is atomic for visibility, but this does not
promise crash durability or preserved permissions.
