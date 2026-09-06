# rename

newlib's `rename` calls `_rename_r`, which is `_link_r` plus `_unlink_r`. FAT
has no hard links, so `_link` can never work and this can never be fixed from
the libc side. It needs a real kernel operation, syscall 25, and newlib's
`rename.o` and `renamer.o` dropped from our archive copy the way `system.o`
already is.

### What it does on disk

A FAT rename moves a directory entry, never the data. The pieces already
exist in `fat_fs.rs`:

1. `find_entry_by_name(old_parent, old_name)` for the source.
2. `prepare_name_entries` + `persist_new_entry` to write a new entry in the
   destination directory carrying the **same** first cluster, size,
   attributes and timestamps, under the new name.
3. Mark the source entry and its long filename entries deleted.

Step 3 is `delete_directory_entry` minus the `free_chain`, so that function
splits into `clear_directory_entry` (entry only) and the existing one, which
becomes `clear_directory_entry` plus the chain free.

**Order matters.** The new entry is written before the old one is cleared. A
crash in between leaves the file reachable under both names, which is wrong
but recoverable; the other order loses the file outright.

### Semantics

POSIX, as far as FAT allows:

- Renaming a path onto itself succeeds and does nothing.
- An existing destination is replaced, but only when both sides are the same
  kind of thing. File over directory or the reverse fails, and a non-empty
  destination directory fails.
- `.` and `..` cannot be renamed.
- Moving a directory into its own subtree fails. Without that check the
  subtree stops being reachable from the root and its clusters leak.

### The trait question

`Directory` is a per-directory trait and rename names two of them. Every
`FATDirectory` holds its own `DirectoryEntry` and a `Weak<Mutex<FATFileSystem>>`,
so the destination's entry is not reachable through `Arc<dyn Directory>`.

Recommended: add one method exposing the identifier a filesystem uses for a
directory, and have rename take it.

```rust
fn directory_id(&self) -> Option<usize>;
fn rename_entry(&self, name: &str, new_parent: usize, new_name: &str) -> Option<()>;
```

For FAT `directory_id` is the first cluster. It leaks a FAT concept into the
trait, but there is one filesystem and the alternative is `Any` plus
downcasting in every implementation. The syscall resolves both parent
directories with the existing `find_directory_from_path`, takes the basenames,
and calls `rename_entry` on the source parent.

### Directories: in or out

Cross-directory *file* moves need none of the below. Moving a **directory** to
a different parent additionally needs:

- its `..` entry rewritten to the new parent's cluster, and
- a walk up from the destination through `..` to reject a move into its own
  subtree, bounded so a corrupt tree cannot loop forever.

That is where most of the risk is, and `os.rename` does not need it. Worth
deciding rather than assuming.

### Userspace

- `rename` in `userspace/lib/syscalls.c`, and `ar d libc_a-rename.o
  libc_a-renamer.o` in `build.mk` next to the `system.o` line.
- `ulib::rename`, and a `mv` builtin in the shell as the way to exercise it by
  hand.

### Testing

None of this can be checked outside the container, and a mistake writes bad
directory entries rather than failing. Worth running the first time against a
throwaway `disk.img`, with `mdir`/`mcopy` from the host to inspect the result.
