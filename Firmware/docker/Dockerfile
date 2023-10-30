FROM ubuntu:23.04 AS base
ARG RUNNER_VERSION="2.311.0"
WORKDIR /
RUN apt update && apt upgrade -y && env DEBIAN_FRONTEND=noninteractive apt install --no-install-recommends -y curl jq build-essential libssl-dev libffi-dev python3 python3-venv python3-dev python3-pip
RUN curl -O -L https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz
WORKDIR /actions-runner
RUN tar xzf ../actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz
RUN ./bin/installdependencies.sh
RUN useradd -ms /bin/bash github
COPY start-oro-runner.sh start-oro-runner.sh
RUN chmod +x start-oro-runner.sh
RUN chown -R github /actions-runner
USER github
ENTRYPOINT ["./start-oro-runner.sh"]
