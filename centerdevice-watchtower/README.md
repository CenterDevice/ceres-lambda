# CenterDevice Watchtower


## Configuration

### Format

```toml
host = 'localhost:8070'
user = 'bosun'
password = 'bosun'
timeout = 5

[bosun.tags]
tag1 = 'value1'
tag2 = 'value2'

[centerdevice_health]
base_domain = 'centerdevice.de'
```

### Validate Configuration

This crate contains a executable that validates an encrypted configuration file called `validate-config-watchtower`. Please check the help information for details. For decryption valid AWS credentials in environment variables are required. 

In this example, the encrypted as well as the decrypted configurations are printed and checked:

```Bash
cargo run --bin validate-config-scaletower ~INFRA/AWS/staging/logimon/terraform/resources/lambda/packages/config_enc_centerdevice-watchtower.conf -vv
```

You can set the AWS credentials for example using `aws-switchrole` -- see below. In this case, don't forget to paste and eval.

```Bash
aws-switchrole --profile staging@cd --copy
```

