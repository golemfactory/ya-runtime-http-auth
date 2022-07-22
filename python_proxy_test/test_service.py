import requests


headers = {'Content-type': 'application/json'}

data = {
  "name": "bor-service",
  "description": "BOR service",
  "serverName": ["127.0.0.1"],
  "bindHttps": "127.0.0.1:1443",
  "bindHttp": "127.0.0.1:1180",
  "from": "/",
  "to": "https://rpc-mainnet.maticvigil.com/v1/fd04db1066cae0f44d3461ae6d6a7cbbdd46e4a5",
  "cert": {
    "path": "c:/certs/server.cert",
    "keyPath": "c:/certs/server.key"
  }
}
r = requests.post("http://127.0.0.1:6668/services", json=data, headers=headers)

data = {
  "username": "uu",
  "password": "pp",
}
r = requests.post("http://127.0.0.1:6668/services/bor-service/users", json=data, headers=headers)

print(r.content)
r = requests.get("http://127.0.0.1:6668/services")
print(r.content)

data = {
  "username": "uu",
  "password": "pp",
}
headers = {'Content-type': 'application/json'}

r = requests.get("https://uu:pp@127.0.0.1:1443", verify=False, headers=headers)
print(r.content)