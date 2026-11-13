# ferrocloud as an image: the one static program, nothing else.
#
# Built from a folder holding `ferrocloud-amd64` and `ferrocloud-arm64`,
# the release builds for x86_64-unknown-linux-musl and
# aarch64-unknown-linux-musl (see .github/workflows/cloud.yml).
FROM scratch
ARG TARGETARCH
COPY ferrocloud-${TARGETARCH} /ferrocloud
# Data is kept here: mount a volume to keep it between containers.
ENV FERROCLOUD_DATA=/var/lib/ferrocloud
VOLUME /var/lib/ferrocloud
EXPOSE 4566
HEALTHCHECK --interval=5s --timeout=3s --start-period=2s --retries=5 CMD ["/ferrocloud", "--check"]
ENTRYPOINT ["/ferrocloud"]
