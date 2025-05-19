# Documentation for Ankaios

To be published on <https://eclipse-ankaios.github.io/ankaios/>.

## Build

```shell
../tools/generate_docs.sh serve
```

and visit <http://127.0.0.1:8000>. Make sure to forward the port in case you are using the dev container.

To have a faster build, you may want to disable the [htmlproofer plugin](https://github.com/manuzhang/mkdocs-htmlproofer-plugin):

```shell
export ENABLED_HTMLPROOFER=false
../tools/generate_docs.sh serve
```
