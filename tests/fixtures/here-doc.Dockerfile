# ------------------------------------------------------------------------------
# Refs: https://github.com/moby/buildkit/blob/fc4876958f956f9eea639d1794ee19b7136bf229/frontend/dockerfile/instructions/parse_heredoc_test.go

FROM ubuntu

ADD /foo /bar
ADD <<EOF /bar
EOF
ADD <<EOF /bar
TESTING
EOF
ADD <<-EOF /bar
	TESTING
EOF
ADD <<'EOF' /bar
TESTING
EOF
ADD <<EOF1 <<EOF2 /bar
this is the first file
EOF1
this is the second file
EOF2
ADD <<EOF foo.txt /bar
this is inline
EOF
ADD <<EOF /quotes
"quotes"
EOF

COPY /foo /bar
COPY <<EOF /bar
EOF
COPY <<EOF /bar
TESTING
EOF
COPY <<-EOF /bar
	TESTING
EOF
COPY <<'EOF' /bar
TESTING
EOF
COPY <<EOF1 <<EOF2 /bar
this is the first file
EOF1
this is the second file
EOF2
COPY <<EOF foo.txt /bar
this is inline
EOF
COPY <<EOF /quotes
"quotes"
EOF

RUN ["ls", "/"]
RUN ["<<EOF"]
RUN ls /
RUN <<EOF
ls /
whoami
EOF
# TODO
RUN <<'EOF' | python
print("hello")
print("world")
EOF
RUN <<-EOF
	echo test
EOF

# ------------------------------------------------------------------------------
# Refs: https://github.com/moby/buildkit/blob/fc4876958f956f9eea639d1794ee19b7136bf229/frontend/dockerfile/parser/parser_heredoc_test.go#L149

FROM alpine:3.6

ENV NAME=me

RUN ls

# Tested in failure test in test.rs.
# USER <<INVALID
# INVALID

RUN <<EMPTY
EMPTY

# TODO: Tested in failure test in test.rs.
# RUN 3<<EMPTY2
# EMPTY2

RUN "<<NOHEREDOC"

RUN <<INDENT
	foo
	bar
INDENT

RUN <<-UNINDENT
	baz
	quux
UNINDENT

RUN <<-UNINDENT2
	baz
	quux
	UNINDENT2

RUN <<-EXPAND
	expand $NAME
EXPAND

RUN <<-'NOEXPAND'
	do not expand $NAME
NOEXPAND

RUN <<COPY
echo hello world
echo foo bar
COPY

RUN <<COMMENT
# internal comment
echo hello world
echo foo bar # trailing comment
COMMENT

RUN --mount=type=cache,target=/foo <<MOUNT
echo hello
MOUNT

COPY <<FILE1 <<FILE2 /dest
content 1
FILE1
content 2
FILE2

COPY <<EOF /quotes
"foo"
'bar'
EOF

COPY <<X <<Y /dest
Y
X
X
Y

# TODO
RUN <<COMPLEX python3
print('hello world')
COMPLEX

COPY <<file.txt /dest
hello world
file.txt

# TODO: Tested in failure test in test.rs.
# RUN <<eo'f'
# echo foo
# eof

# TODO: Tested in failure test in test.rs.
# RUN <<eo\'f
# echo foo
# eo'f

# TODO: Tested in failure test in test.rs.
# RUN <<'e'o\'f
# echo foo
# eo'f

# TODO: Tested in failure test in test.rs.
# RUN <<'one two'
# echo bar
# one two

# TODO: Tested in failure test in test.rs.
# RUN <<$EOF
# $EOF
