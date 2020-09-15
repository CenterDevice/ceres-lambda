# AWS-Scaletower


## Configuration

### Format

```toml
[bosun]
host = '<Bosun host:Bosun port>'
user = '<Basic Auth user name'
password = '<Basic Auth password | KMS encrypted and base64 encoded'
timeout = 3

[bosun.tags]
iaas_name = 'aws'
iaas_account = 'staging'
iaas_env = 'staging'

[duo]
api_host_name = "apixxxxx.duo.com"
integration_key = "123456789ABCDEF"
secret_key = "WouldYouWant2Know?"

[credentials]
disable_threshold_days = 60
delete_threshold_days = 180
actions_enabled = false
whitelist = [
    "aws:api_key:12345",
    "aws:password:23456",
    "duo:tfa::34567"
]
```

### Validate Configuration

This crate contains a executable that validates an encrypted configuration file called `validate-config-security-watchtower`. Please check the help information for details. For decryption valid AWS credentials in environment variables are required. 

In this example, the encrypted as well as the decrypted configurations are printed and checked:

```Bash
cargo run --bin validate-config-security-watchtower ~INFRA/AWS/staging/logimon/terraform/resources/lambda/packages/config_enc_security-watchtower.conf -vv
```

You can set the AWS credentials for example using `aws-switchrole` -- see below. In this case, don't forget to paste and eval.

```Bash
aws-switchrole --profile staging@cd --copy
```

