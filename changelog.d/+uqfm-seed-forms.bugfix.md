`DEEDAR_HOST_SIGNING_KEY` takes a seed written `seed:` then 32 bytes, or `hex:` then 64 hex digits.
A seed with `/` or `\` in it is now writable, where bare it was refused as a path.
A bare 32-byte value with a separator is still a path, and its refusal names both forms.
The reference lists the forms in the order they are read.
