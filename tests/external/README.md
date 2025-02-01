# External repositories used for tests

This directory contains the following repositories as submodules:

- [buildah](buildah): <https://github.com/containers/buildah>, licensed under "Apache-2.0".
- [buildkit](buildkit): <https://github.com/moby/buildkit>, licensed under "Apache-2.0".
- [moby](moby): <https://github.com/moby/moby>, licensed under "Apache-2.0".

Each corresponding directory under [dump](dump) dictory contains files derived from these.

These are used for testing and benchmarking only and are not included in the code published on crates.io.

See the license files included in these directories for license information.
