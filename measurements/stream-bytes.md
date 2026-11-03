# Product bytes streamed in chunks: peak RSS and wall time

`deedar create file`, `evidence` and `export` used to read an artefact whole
before hashing or copying it, so each held the file's full size in memory.
They now read it 256 KiB at a time (`CHUNK` in `crates/deedar/src/fs.rs`).

- CPU: Intel(R) Xeon(R) Processor, family 6 model 207, 8 cores, 2 MiB L2 per
  core, SHA-NI. 47 GiB RAM, Linux 6.12, ext4. CPU only.
- Before: `0bca3d786c37ff497b923eddc6bd55a1a0f6afee` (main).
  After: that commit with this change.
- Built with `cargo build --release -p deedar-cli`, Rust 1.97.1.
- Run with `RUNS=5 measurements/stream-bytes.sh BEFORE AFTER`. Each run is a
  fresh store; the figures are medians of 5. Peak RSS is GNU time's `%M`.
  The artefact is random bytes and sits in the page cache.

## Before and after

| MiB  | command  | peak RSS before (KiB) | after (KiB) | wall before (ms) | after (ms) |
|------|----------|----------------------:|------------:|-----------------:|-----------:|
| 10   | create   |                13 296 |       3 348 |               17 |         12 |
| 10   | evidence |                13 204 |       3 304 |               12 |          9 |
| 10   | export   |                13 232 |       3 320 |               16 |         12 |
| 100  | create   |               105 500 |       3 376 |              145 |        103 |
| 100  | evidence |               105 380 |       3 300 |               99 |         70 |
| 100  | export   |               105 420 |       3 256 |              146 |        102 |
| 1024 | create   |             1 051 620 |       3 432 |            1 399 |        992 |
| 1024 | evidence |             1 051 544 |       3 320 |            1 012 |        698 |
| 1024 | export   |             1 051 564 |       3 248 |            1 825 |      1 069 |

Before, peak RSS is the file size plus a ~3 MB process baseline. After, it is
the baseline whatever the size. Wall time falls too, because nothing has to
fault in a buffer the size of the file.

## Chunk size

These binaries were built from the after tree with `CHUNK` edited. They were
run with `SIZES=1024` on the 1 GiB artefact, first with `RUNS=5` over every
size, then with `RUNS=9` over five of them. Peak RSS is from `create`; for
`evidence` and `export` it is within 200 KiB of that.

| chunk   | create (ms) | evidence (ms) | export (ms) | peak RSS (KiB) |
|---------|------------:|--------------:|------------:|---------------:|
| 8 KiB   | 1115 / 1056 |     712 / 689 | 1428 / 1373 |    3244 / 3244 |
| 16 KiB  |        1053 |           694 |        1385 |           3072 |
| 32 KiB  |        1055 |           690 |        1355 |           3244 |
| 64 KiB  | 1051 / 1005 |     696 / 680 | 1380 / 1338 |    3220 / 3368 |
| 128 KiB | 1042 / 1031 |     688 / 682 | 1359 / 1342 |    3244 / 3312 |
| 256 KiB | 1005 / 1035 |     689 / 681 | 1333 / 1363 |    3328 / 3376 |
| 1 MiB   | 1014 / 1060 |     689 / 683 | 1337 / 1334 |    4136 / 4144 |
| 4 MiB   |        1037 |           704 |        1346 |           7260 |

Hashing dominates here. `evidence` reads 1 GiB in about 690 ms, which is
SHA-256 at about 1.5 GB/s. The chunk size moves wall time by no more than the
spread between the two runs, though 8 KiB was the slowest `evidence` both
times. Peak RSS stays at the baseline through 256 KiB and grows with the
buffer above it. 256 KiB is the largest chunk that costs no memory here. It
is also the size GNU coreutils measured as the least syscall overhead
(`src/ioblksize.h`), and it is larger than Linux's default 128 KiB
readahead window, so the page cache is read in fewer calls than it is filled.
