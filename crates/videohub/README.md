# videohub
Implementation of the Blackmagic Videohub Ethernet Protocol.

Not only should this handle what is documented in [Developer Information for Videohub][1], but also real life partially-incorrect uses and undocumented fields.

This implements the parsing and serialization to modelled messages along with an optional tokio-utils Codec,
but the logic of how to interpret these messages is up to the user.

# See Also
- [Videohub Developer Information][1]
- [videohubctrl][2]

[1]: https://documents.blackmagicdesign.com/DeveloperManuals/VideohubDeveloperInformation.pdf
[2]: https://github.com/gfto/videohubctrl
