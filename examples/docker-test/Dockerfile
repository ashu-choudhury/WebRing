# Sample Dockerfile for WebRing Container Build
FROM alpine:3.20

WORKDIR /app
COPY message.txt /app/message.txt

ENV APP_NAME="WebRing-Custom-Container"
ENTRYPOINT ["/bin/busybox"]
CMD ["busybox", "cat", "/app/message.txt"]
