# Resolving a `sha256:` from the log

`Client::resolve` on a `sha256:` used to read and decode every deed, then hash
each one. The log already stores the deed digest next to the accession, so a
digest that is present is now read from that file. One matching frame is
hashed again, and a tombstone in the log is not a hit. A product path, and a
deed the log has not caught up to, still walk the shelf.

- CPU: Intel(R) Xeon(R) Processor, family 6 model 207, stepping 2, 8 cores,
  1 thread per core, SHA-NI, 2 MiB L2 per core, 320 MiB L3. 47 GiB RAM,
  Linux 6.12.94+, ext4, KVM. CPU only.
- Before: `0e6e22481704b16379c878b71c4408175d35ed9d`. After: that commit with
  this change.
- rustc 1.97.1 (8bab26f4f 2026-07-14). Release build,
  `cargo bench -p deedar --bench resolve --no-run`.
- Each run is a fresh process of that binary:
  `--bench --color never --sample-size 80`. Six runs of the shelf walk and
  six of the log lookup, alternating, shelf walk first. The number kept from
  a run is criterion's point estimate. The median is the mean of the 3rd and
  4th of the sorted six. The IQR is the gap between the medians of the lower
  three and the upper three.

The store is `n` quote deeds plus one file deed, built in the process and then
only looked up. A quote has no product path, so the file is that variant. The
present digest is the quote at `n/2`. The absent digest is 32 bytes of `0xab`,
which this store did not write. Criterion warms up for 3 s and measures for
5 s, so the lookup runs against a store that process just wrote.

Criterion's default is 100 samples. One release run of a miss over 10 000
deeds took 59.3 ms, and criterion warned that 100 samples of it need 6.0 s or
else 80 samples. Eighty samples of that miss are 4.7 s, inside the 5 s
measurement, so every lookup uses 80. None of these twelve runs warned.

## Before and after

| deeds | lookup | before (µs) | IQR | after (µs) | IQR |
|------:|--------|------------:|----:|-----------:|----:|
| 100 | present | 408.8 | 12.7 | 9.13 | 0.18 |
| 100 | absent | 410.1 | 2.5 | 416.6 | 2.4 |
| 100 | product | 408.8 | 4.6 | 416.8 | 1.0 |
| 1 000 | present | 4 372 | 44 | 53.33 | 0.15 |
| 1 000 | absent | 4 376 | 44 | 4 436 | 6.8 |
| 1 000 | product | 4 365 | 43 | 4 447 | 52 |
| 10 000 | present | 47 420 | 841 | 507.7 | 6.3 |
| 10 000 | absent | 47 216 | 801 | 47 832 | 2 718 |
| 10 000 | product | 47 170 | 923 | 47 802 | 634 |

A present digest is 45 times faster at 100 deeds, 82 times at 1 000, and 93
times at 10 000. The shelf walk was the same cost whichever digest was asked
for, because every one of them decoded every frame. The log lookup does not.

A miss and a product path still walk the shelf, after one read of the log.
At 100 deeds that read adds about 7 µs to a 410 µs walk, and the two medians
sit apart by more than either IQR. At 1 000 deeds the miss is about 60 µs
slower and the product path about 80 µs, on a 4.4 ms walk. At 10 000 deeds
the medians differ by 0.6 ms, and the IQR of the miss after the change is
2.7 ms, so those twelve runs do not separate the miss from the old walk.

The first of the twelve processes was the slow one at 10 000 deeds (50.5 ms
present before, 52.5 ms absent before). The median is what drops it.

## The six point estimates, in run order (µs)

```
present/100    before  409.17 428.29 404.86 404.42 417.52 408.38
present/100    after     9.178  9.005  9.082  9.059  9.292  9.235
absent/100     before  410.40 408.23 412.73 406.75 410.75 409.85
absent/100     after   416.25 414.49 416.37 418.62 416.77 423.34
product/100    before  409.23 408.44 407.27 406.71 411.87 414.00
product/100    after   417.55 416.77 416.51 416.81 416.36 426.67
present/1000   before  4380.9 4360.3 4364.0 4352.4 4414.6 4403.9
present/1000   after    54.077 53.360 53.297 53.220 52.926 53.365
absent/1000    before  4425.6 4351.7 4344.8 4356.9 4396.0 4395.1
absent/1000    after   4434.0 4381.4 4437.5 4437.7 4430.9 4464.0
product/1000   before  4405.3 4355.6 4344.8 4357.3 4399.0 4372.2
product/1000   after   4505.6 4416.2 4457.0 4437.4 4404.3 4468.1
present/10000  before  50542 46952 46838 47048 47791 47793
present/10000  after   512.30 508.14 503.42 514.27 507.28 505.99
absent/10000   before  52499 46849 46867 46949 47668 47482
absent/10000   after   50247 47330 47529 47576 48087 51500
product/10000  before  49750 46827 46983 46739 47750 47356
product/10000  after   47841 47341 47763 47467 48131 48101
```
