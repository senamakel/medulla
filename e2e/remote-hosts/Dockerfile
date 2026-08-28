# One image, two roles. The client and the host run the same binary — a remote
# host *is* a medulla — so building twice would only create somewhere for the
# two to drift.
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
      openssh-server openssh-client ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /run/sshd

# A throwaway key, generated at build time and shared by both containers. It
# never leaves this image and the image is never published: it exists so the
# suite can run unattended, and for no other reason.
RUN ssh-keygen -t ed25519 -N "" -f /root/.ssh/id_ed25519 -q \
    && cp /root/.ssh/id_ed25519.pub /root/.ssh/authorized_keys \
    && chmod 600 /root/.ssh/authorized_keys

# Host-key checking off, because the host key is minted fresh on every run and
# there is no prior trust to establish. This is a disposable test network, not a
# pattern to copy.
RUN printf 'StrictHostKeyChecking no\nUserKnownHostsFile /dev/null\nLogLevel ERROR\n' \
      >> /etc/ssh/ssh_config \
    && sed -i 's/^#*PermitRootLogin.*/PermitRootLogin prohibit-password/' /etc/ssh/sshd_config

# The binary under test, built on the host and mounted in by compose.
ENV PATH="/opt/medulla:${PATH}"
WORKDIR /work
