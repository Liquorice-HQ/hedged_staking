#!/bin/bash -e
## Removes created containers and volumes

docker-compose down
docker-compose down --volumes

# docker volume ls --format '{{.Name}}' --filter 'Name=collector'
