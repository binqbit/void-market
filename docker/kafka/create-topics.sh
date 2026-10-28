#!/usr/bin/env bash
set -euo pipefail

broker=$1
shift

for topic in "$@"; do
    /opt/kafka/bin/kafka-topics.sh \
        --bootstrap-server "$broker" \
        --create \
        --if-not-exists \
        --topic "$topic" \
        --partitions 1 \
        --replication-factor 1
done
