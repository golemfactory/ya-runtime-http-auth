# ya-runtime-http-auth

`ya-runtime-http-auth` is a Yagna runtime binary for advertising HTTP-based services on the Golem Network.

---

Quick links:
- [Overview](#overview)
- [Runtime proposal document](https://github.com/golemfactory/golem-architecture/blob/7efb7fb7207980ba2dccb735e676f8436dcf18d8/gaps/gap-8_http_auth_runtime/gap-8_http_auth_runtime.md)
- [Provider Agent - advertising a service](#provider-agent---advertising-a-service)
- [Requestor Agent - overview](#requestor-agent---overview)
- [Requestor Agent - example code using yapapi](https://github.com/golemfactory/yapapi/blob/mf/http-auth-example/examples/http-auth/http_auth.py)
- [Self-signed certificates](#self-signed-certificates)

---


## Overview

`ya-runtime-http-auth` serves as a gateway between the Golem Marketplace and an HTTP-based service accessible over
the Internet.

```
                                +-------------- Provider's machine --------------+
+-----------+   Golem Network   | +----------+      +---------+      +---------+ |
| Requestor | <=================> | Provider | <--> | ExeUnit | <--> | Runtime | |
+-----------+                   | +----------+      +---------+      +---------+ |
                                +------------------------------------------------+
```

All HTTP requests to the service are routed via a custom reverse HTTP proxy implementation.
The proxy authorizes users and collects per-user, per-endpoint usage statistics.
These statistics will be used for billing purposes and sent to the Requestor.

The runtime binary is responsible for managing users authorized to use the service. Requestor's commands 
are translated into proxy's Management API calls, upon prior identity verification.

```
-------------- Provider's machine -----------+
  +---------+   Management API   +---------+ |     Internet      +------+
  | Runtime | <----------------> |  Proxy  | <=================> | User |
  +---------+                    +---------+ |                   +------+
                                     ||      |
                                 +---------+ |
                                 | Service | |
                                 +---------+ |
---------------------------------------------+                                                      

```

Runtime specification proposal can be found [here](https://github.com/golemfactory/golem-architecture/blob/7efb7fb7207980ba2dccb735e676f8436dcf18d8/gaps/gap-8_http_auth_runtime/gap-8_http_auth_runtime.md).

---

## Provider Agent - advertising a service

At the moment, it is required from the user to manually perform the setup steps below. 
This process will be largely automated in the future.

### Preparing your service

1. Ensure that the service is listening on a local socket (a private IP address).

2. Configure a daemon supervisor for your service. In case of a crash, it should be automatically restarted.

3. The service needs to be running when advertised on the Golem Network.

Please note that if your HTTP service requires some additional authorization (e.g. user certificates), it may not be supported by
`ya-runtime-http-auth` in the current version.

### Installation

1. `yagna`

  In order to install yagna, please refer to this [handbook chapter](https://handbook.golem.network/provider-tutorials/provider-tutorial).

2. `ya-runtime-http-auth`

  Download and install the latest `deb` package from the [releases page](https://github.com/golemfactory/ya-runtime-http-auth/releases/latest).
  You will find the installed runtime and proxy binaries at the `/usr/lib/yagna/plugins` directory.

### Service definition

Service definition files contain basic information on the service and the configuration of the proxy HTTP server.
There can be multiple services exposed by a single server as long as they are configured with the distinct `from` endpoints.

The definition files are, by default, located at `~/.local/share/ya-runtime-http-auth/services`. 
Create the path by typing the following command in a terminal:

```bash
mkdir -p ~/.local/share/ya-runtime-http-auth/services
```

Now, save this service definition file called `acme-service.json` at the newly created location:

```json
{
  "name": "acme-service",
  "description": "ACME service v1.42",
  "serverName": ["service.acme.com", "1.2.3.4"],
  "bindHttps": "0.0.0.0:443",
  "bindHttp": "0.0.0.0:80",
  "from": "/acme",
  "to": "http://127.0.0.1:10000",
  "cert": {
    "path": "/secure/acme/certs/server.cert",
    "keyPath": "/secure/acme/certs/server.key"
  }
}
```

- `name` - name of the service
- `description` - extended service information
- `serverName` - list of assigned domain names and / or public IP addresses
- `bindHttps` - address to bind the HTTPS server to (required if `bindHttp` is not set)
- `bindHttp` - address to bind the HTTP server to (required if `bindHttps` is not set)
- `from` - source service endpoint. In this case, `service.acme.com/acme` or `1.2.3.4/acme`
- `to` - service listening URL
- `cert` - certificate and private key paths (required for HTTPS)

In this example, all requests from e.g. `https://1.2.3.4/acme/register` will be redirected to `http://127.0.0.1:10000/register`.

**It's not recommended to use an HTTP-only proxy server for the service**. Unencrypted credentials sent by the users can
be captured by malicious actors in their local networks. Please create and use self-signed certificates when facing real-world 
users. You might find the [following chapter](#self-signed-certificates) helpful.

### Provider configuration

### Runtime definition

Each advertised service acts as a separate runtime and requires a new descriptor file, located at

a. `~/.local/lib/yagna/plugins` when using `golemsp`
b. `/usr/lib/yagna/plugins` when running the `ya-provider` binary directly

Runtime definition file's name needs to match the `ya-*.json` pattern to be discovered by the Provider Agent. In this case, 
the file will be called `ya-runtime-acme.json` and contain the following:

```json
[
  {
    "name": "acme-service",
    "version": "0.1.0",
    "supervisor-path": "exe-unit",
    "runtime-path": "ya-runtime-http-auth/ya-runtime-http-auth",
    "extra-args": [
      "--runtime-managed-image",
      "--runtime-arg", "acme-service"
    ],
    "config": {
    	"counters": {
    	  "http-auth.requests": {
            "name": "requests",
            "description": "Total number of HTTP requests",
            "price": true
    	  }
    	}
    }
  }
]
```

- `name` - name of the service, advertised in the Golem Network
- `version` - service version
- `supervisor-path` - path to ExeUnit Supervisor, most often located in the same directory
- `extra-args` - extra arguments passed to the ExeUnit Supervisor
  - `--runtime-managed-image` - the Supervisor will not be responsible for downloading an image / payload to be executed by the Runtime
  - `--runtime-arg acme-service` - the Runtime will look for a service definition file with a name set to `acme-service`
- `config` -> `counters`
  - `http-auth.requests` - defines the service's HTTP request counter by `ya-runtime-http-auth`. `"price": true` 
    informs the Supervisor that this counter will be used in calculating the price. The counter only includes users
    created by the current Requestor
    
### Billing configuration

In order to advertise the newly-created service in the Golem Network, a billing profile needs to be created. This can be
achieved by editing the Provider Agent's presets file (`~/.local/share/ya-provider/presets.json`) to include the following:

```json
{
  "ver": "V1",
  "active": [
    "acme"
  ],
  "presets": [
    {
      "name": "acme",
      "exeunit-name": "acme-service",
      "pricing-model": "linear",
      "initial-price": 0,
      "usage-coeffs": {
        "golem.usage.duration_sec": 0.001,
        "golem.usage.cpu_sec": 0.001,
        "http-auth.requests": 0.00001
      }
    }
  ]
}
```

This configuration file contains a single active preset called `acme`, defined for the runtime `acme-service`, 
as stated in the `ya-runtime-acme.json` definition file. Each HTTP call made by an authorized user will 
cost the Requestor 0.00001 GLM (or tGLM when running on the test network).

### Starting the provider

The configuration process is complete. Start your provider by typing `golemsp run` in the terminal.

## Requestor Agent - overview

[This link](https://github.com/golemfactory/yapapi/blob/mf/http-auth-example/examples/http-auth/http_auth.py) will point you
to the minimal implementation of an HTTP service advertised on Golem Marketplace.

However, the real-world implementation would contain:
- a custom market strategy that takes the HTTP request price into account
- code to constrain the `golem.runtime.http-auth.https` property in the Offer to `true`.
  This way Requestors enable their users to establish secure HTTPS connections with the service.
- the `service info` command outputs a certificate hash, which should be used by clients to verify certificate's contents 

## Self-signed certificates

In most cases, a provider's machine won't be addressable by a domain name and their certificate won't be signed by a trusted authority.
When the server presents a self-signed certificate, users will only be able to verify the embedded signature.

However, the Requestor can equip each user with a certificate hash returned by the `service info` runtime command. 
Each user's HTTPS client may verify the certificate's hash, so that Man-in-the-Middle attacks can be prevented.
The client should ignore the missing Certificate Authority signature and the domain name included in the certificate.

### Creating a self-signed certificate with OpenSSL

Currently application do not support keys encrypted with des. Use nodes options to generate unecrypted private key.

```bash
openssl req -nodes -x509 -newkey rsa:4096 -keyout server.key -out server.cert -sha256 -days 3650
```
