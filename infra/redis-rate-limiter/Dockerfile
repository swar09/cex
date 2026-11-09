FROM redis:7-alpine

WORKDIR /data

COPY redis.conf /usr/local/etc/redis/redis.conf
COPY scripts/ /usr/local/share/redis/scripts/

EXPOSE 6379

CMD ["redis-server", "/usr/local/etc/redis/redis.conf"]
