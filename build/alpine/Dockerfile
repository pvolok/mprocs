FROM ocamlpro/ocaml:4.12
ADD repositories /etc/apk/repositories
RUN sudo apk update && \
    sudo apk add \
      libtool \
      perl \
      upx \
      zip \
      rust cargo
RUN opam switch create ocaml-system && \
    opam install dune cmdliner containers-data logs logs-ppx \
      lwt lwt_ppx ppx_deriving ppx_jane stdio yojson

WORKDIR /app
