FROM apache/kafka:4.1.2

USER root
RUN mkdir -p /var/lib/kafka/data \
    && chown appuser:root /var/lib/kafka/data

COPY --chmod=755 create-topics.sh /opt/kafka/create-topics.sh

USER appuser
