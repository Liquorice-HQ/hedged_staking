FROM amazonlinux:2

RUN amazon-linux-extras enable python3.8
RUN yum install gcc --assumeyes
RUN yum install python38 --assumeyes
RUN yum install python38-devel --assumeyes
RUN pip3.8 install web3 toml dydx-v3-python toml
RUN ln --symbolic /usr/bin/python3.8 /usr/bin/python3

RUN mkdir /vf-hedgestaking-bin
RUN mkdir /log
RUN mkdir /workdir

COPY hedged_staking /vf-hedgestaking-bin
COPY eth-operations.py /vf-hedgestaking-bin

ENV PYTHONPATH=/vf-hedgestaking-bin

# for Prometheus:
EXPOSE 8080/tcp
WORKDIR /workdir
ENTRYPOINT ["/vf-hedgestaking-bin/hedged_staking"]

