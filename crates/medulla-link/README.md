# medulla-link

Mosh-style host-link transport for direct UDP synchronization between a Medulla client and remote host. The crate also retains a blind UDP forwarder route for coordination tests; it is not a deployed bootstrap. The protocol and wire guarantees are described in the [host link protocol](../../gitbooks/developers/host-link-protocol.md).

This standalone crate does not depend on the SDK or terminal application. See the [workspace architecture](../../gitbooks/developers/architecture.md) for the complete crate map.
