FROM apache/kafka:4.1.2

USER root
RUN mkdir -p /var/lib/kafka/data \
    && chown appuser:root /var/lib/kafka/data

USER appuser
