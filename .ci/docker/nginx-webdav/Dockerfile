FROM ubuntu:22.04

RUN apt-get update && apt-get dist-upgrade -y && apt-get install -y nginx-extras

COPY ./.ci/docker/nginx-webdav/webdav.conf /etc/nginx/conf.d/default.conf
RUN rm -rf /etc/nginx/sites-enabled/*

RUN mkdir -p "/media/data"

RUN chown -R www-data:www-data "/media/data"

VOLUME /media/data

CMD nginx -g "daemon off;"
