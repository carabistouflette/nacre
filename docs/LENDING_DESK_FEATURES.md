# Lending desk feature coverage

The desk is a behavior-focused companion to the V9 map API. It intentionally
uses only integer-keyed map lookup/update/removal and a separate ordered ID list;
it does not add map iteration or another key type.

| Feature or invariant | Implementation: file and functions | Coverage |
|---|---|---|
| Shared availability and owned resource data | `examples/lending_desk/model.ncr`: `Availability`, `ResourceData` | `tests/lending_desk_v9.rs`: available, lent, maintenance, and release transitions across books/tools |
| Owned interface upcast and polymorphic models | `model.ncr`: `Book`, `Tool`, `Loanable`; `desk.ncr`: `addBook`, `addTool`, `printPreview`, `displayResource` | `catalog_commands_preserve_polymorphism_relationships_and_stable_ids` checks both resource kinds and their distinct details |
| Receiver-derived shared/exclusive resource access | `model.ncr`: `getLabel`, `getLabelMut`, `getAvailability`; `catalog.ncr`: `getRef`, `getMut`; `desk.ncr`: `displayResource`, `renameResource`, `lendResource`, `returnResource`, `setMaintenance` | The behavior test reads sibling catalogs together, replaces labels, appends through an exclusive label borrow, and updates availability only after shared loans end |
| Generic map plus insertion order | `examples/lending_desk/catalog.ncr`: `Catalog<T>.put`, `getRef`, `getMut`, `remove`, `idAt` | Persistence snapshots and command tests check stable insertion order after deleting an earlier resource and adding a later ID |
| V8 lexical list traversal under shared catalog loans | `catalog.ncr`: `idsRef`, `resourceIdsRef`, `memberIdsRef`; `desk.ncr`: `listResources`, `listMembers` | Lists stable IDs once per traversal, performs shared sibling-map lookup, and ends the whole-list loan before later commands mutate catalogs |
| Members, optional notes, and active-loan references | `model.ncr`: `Member`; `catalog.ncr`: `DeskState.hasActiveLoanFor`, `hasValidRelationships`; `desk.ncr`: `displayMember`, `removeMember` | Behavior and malformed-load tests cover note presence, removal rejection for an active borrower, and missing-member reference rejection |
| Availability state guards | `desk.ncr`: `lendResource`, `returnResource`, `setMaintenance`, `removeResource` | Behavior test checks lending guards, lent-resource maintenance/removal rejection, successful return, maintenance entry/release, and return rejection when not lent |
| Checked logical day and due-date arithmetic | `catalog.ncr`: `DeskState.logicalDay`, `setLogicalDay`; `desk.ncr`: `lendResource`, `advanceDay` | `overflowing_day_and_due_date_leave_a_loaded_state_unchanged` checks that due-date and day overflow preserve the saved state |
| Transactional command prompts and custom dispatch | `desk.ncr`: `Command`, `parseCommand`, `prompt`, `promptInteger`, `handleCommand`, command handlers | `catalog_commands_preserve_polymorphism_relationships_and_stable_ids` exercises all 17 commands; `invalid_prompt_values_do_not_commit_partial_resources` covers invalid input without consuming IDs |
| Local borrowed parsing view and UTF-8 byte lengths | `persistence.ncr`: `Reader`, `parseDesk` | `malformed_loads_leave_the_active_desk_byte_for_byte_unchanged` covers truncation, invalid note flags, UTF-8 boundary failure, overflowing metadata, invalid relationships, duplicate IDs, and trailing data |
| Candidate construction and relationship validation | `persistence.ncr`: `parseDesk`; `catalog.ncr`: `DeskState.hasValidRelationships`; `desk.ncr`: `loadPath` | `malformed_loads_leave_the_active_desk_byte_for_byte_unchanged` compares snapshots before/after every failed load |
| Complete versioned persistence and atomic save | `persistence.ncr`: `encodeDesk`, `parseDesk`; `desk.ncr`: `loadPath`, `savePath`; `main.ncr`: optional `Args` startup path | Native regressions compare meaningful byte-for-byte records and reload from paths containing spaces and Unicode |
| Invalid root aliasing | `examples/invalid/lending_desk_map_root_alias.ncr` | Rejected companion holds a map value borrow during insertion into that map |
| Invalid local view escape | `examples/invalid/lending_desk_view_escape.ncr` | Rejected companion returns a reader view whose borrowed input is local |
| Invalid borrowed owner transfer | `examples/invalid/lending_desk_borrowed_owner_transfer.ncr` | Rejected companion passes a borrowed resource view to an owner-taking parameter |

The authoritative command prompts, domain rules, exact persistence fields and
atomicity limitations are documented in `docs/LENDING_DESK.md`.
