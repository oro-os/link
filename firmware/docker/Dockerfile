FROM ubuntu:23.04 AS base
RUN apt update && apt upgrade -y && env DEBIAN_FRONTEND=noninteractive apt install --no-install-recommends -y curl jq build-essential libssl-dev libffi-dev python3 python3-venv python3-dev python3-pip socat

FROM base AS link-test
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain nightly
RUN echo 'export PATH="$PATH:$HOME/.cargo/bin"' >> $HOME/.bashrc
WORKDIR /oro-link-x86
COPY . ./
RUN bash -l -c 'cargo build --release -p link-test'
RUN cp target/release/link-test /link-test

FROM base
ARG RUNNER_VERSION="2.311.0"
WORKDIR /
RUN curl -O -L https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz
WORKDIR /actions-runner
RUN tar xzf ../actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz
RUN ./bin/installdependencies.sh
RUN useradd -ms /bin/bash github
RUN mkdir /oro
COPY docker/start-oro-runner.sh start-oro-runner.sh
RUN chmod +x start-oro-runner.sh
RUN chown -R github /actions-runner /oro
COPY --from=link-test /link-test /usr/bin/link-test
RUN chmod -R a+rwx /usr/bin/link-test /oro
USER github
ENTRYPOINT ["./start-oro-runner.sh"]
