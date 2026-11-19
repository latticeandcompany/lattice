FROM node:latest AS node

FROM rust:latest

COPY --from=node /usr/local/ /usr/local/

RUN apt-get update && apt-get install -y --no-install-recommends \
	libwebkit2gtk-4.1-dev \
	build-essential \
	curl \
	wget \
	file \
	libxdo-dev \
	libssl-dev \
	libayatana-appindicator3-dev \
	librsvg2-dev \
	&& rm -rf /var/lib/apt/lists/*
