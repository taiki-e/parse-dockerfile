arg a=b

from ubuntu as f
add s d
arg a
cmd ["executable"]
copy s d
entrypoint ["executable"]
env k=v
expose 80
healthcheck --interval=30s --timeout=30s --start-period=5s --retries=3 cmd ["executable"]
label k="v"
maintainer name
run a
shell ["executable"]
stopsignal SIGINT
user u
volume ["/dir"]
workdir /dir

onbuild add s d
onbuild arg a
onbuild cmd ["executable"]
onbuild copy s d
onbuild entrypoint ["executable"]
onbuild env k=v
onbuild expose 80
onbuild healthcheck none
onbuild label k="v"
onbuild run a
onbuild shell ["executable"]
onbuild stopsignal SIGINT
onbuild user u
onbuild volume ["/dir"]
onbuild workdir /dir
