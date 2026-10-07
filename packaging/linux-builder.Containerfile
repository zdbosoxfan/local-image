# Ubuntu 24.04 supplies the supported glibc 2.39 / Python 3.12 build baseline.
FROM docker.io/library/ubuntu:24.04@sha256:a853f94d226358a79c740cfc7bce0c289748f3fe3488d921d038ccd752c61b60
ENV DEBIAN_FRONTEND=noninteractive PIP_DISABLE_PIP_VERSION_CHECK=1
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3.12 python3.12-venv python3.12-dev binutils patchelf gcc g++ curl ca-certificates \
    libnss3 libnspr4 libxkbcommon0 libxkbcommon-x11-0 libxcb-cursor0 libxcb-icccm4 \
    libxcb-image0 libxcb-keysyms1 libxcb-render-util0 libxcb-xinerama0 libxcb-xkb1 \
    libxcb1 libxcb-util1 libxcb-shape0 libdbus-1-3 libegl1 libgl1 libopengl0 libasound2t64 \
    libfontconfig1 libxcomposite1 libxdamage1 libxfixes3 libxrandr2 libxtst6 libxi6 \
    libcups2t64 libdrm2 libgbm1 libgtk-3-0t64 libpulse0 xvfb xauth fonts-dejavu-core \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
