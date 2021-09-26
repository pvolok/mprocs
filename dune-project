(lang dune 2.9)

(name mprocs)

(generate_opam_files true)

(package
 (name mprocs)
 (depends
  cmdliner
  containers-data
  dune
  logs
  logs-ppx
  lwt
  lwt_ppx
  ppx_deriving
  ppx_jane
  stdio
  yojson))
