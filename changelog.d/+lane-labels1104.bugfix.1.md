`deedar check` and `deedar vouch check` exit `EXIT_UNTRUSTED_SIGNER` when the signer is not one the reader accepts.
The report and the warning are still printed.
A signature that does not cover the bytes still exits 1.
