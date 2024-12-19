# Test Execution

In a nutshell, test execution in openDuT works by executing containerized (Docker or Podman) test applications on a peer
and uploading the results to a WebDAV directory. Test executors can be configured through either CLEO or LEA.

The container image specified by the `image` parameter in the test executor configuration can either be a
container image already present on the peer or an image remotely available, e.g., in the Docker Hub.

A containerized test application is expected to move all test results to be uploaded to the `/results/` directory
within its container and create an empty file `/results/.results_ready` when all results have been copied there.
When this file exists, or when the container exits and no results have been uploaded yet,
EDGAR creates a ZIP archive from the contents of the `/results` directory and uploads it to the WebDAV server
specified by the `results-url` parameter in the test executor configuration.

In the `testenv` launched by THEO, a WebDAV server is started automatically and can be reached at `http://nginx-webdav/`.  
In the [Local Test Environment](https://github.com/eclipse-opendut/opendut/tree/development/.ci/deploy/localenv),
a WebDAV server is also started automatically and reachable at `http://nginx-webdav.opendut.local`.

Note that the execution of executors is only triggered by deploying the cluster.

## Test Execution using CLEO
In CLEO, test executors can be configured either by passing all configuration parameters as command line arguments...

    $ opendut-cleo create container-executor --help 
    Create a container executor using command-line arguments

    Usage: opendut-cleo create container-executor [OPTIONS] --peer-id <PEER_ID> --engine <ENGINE> --image <IMAGE>

    Options:
        --peer-id <PEER_ID>          ID of the peer to add the container executor to
    -e, --engine <ENGINE>            Engine [possible values: docker, podman]
    -n, --name <NAME>                Container name
    -i, --image <IMAGE>              Container image
    -v, --volumes <VOLUMES>...       Container volumes
        --devices <DEVICES>...       Container devices
        --envs <ENVS>...             Container envs
    -p, --ports <PORTS>...           Container ports
    -c, --command <COMMAND>          Container command
    -a, --args <ARGS>...             Container arguments
    -r, --results-url <RESULTS_URL>  URL to which results will be uploaded
    -h, --help                       Print help

...or by providing the executor as part of a YAML file via `opendut-cleo apply`.
See [Applying Configuration Files](https://opendut.eclipse.dev/book/user-manual/cleo/commands.html#applying-configuration-files) for more information.


## Test Execution Through LEA
In LEA, executors can be configured via the tab `Executor` during peer configuration, using similar parameters as for CLEO.
