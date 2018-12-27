# AWS-Watchtower


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

# ASG Mappings is a list. So multiple items are allowed.
[[asg_mappings.mapping]]
search = '<substring in actual ASG name'
tag_name = '<tag to assign to all scaling events'
host_prefix = '<host prefix to use together with instance id for silences'
```

### Validate Configuration

This crate contains a executable that validates an encrypted configuration file called `validate-config`. Please check the help information for details. For decryption valid AWS credentials in environment variables are required. 

In this example, the encrypted as well as the decrypted configurations are printed and checked:

```Bash
cargo run --bin validate-config ~INFRA/AWS/staging/logimon/terraform/resources/lambda/packages/config_enc.conf -vv
```

You can set the AWS credentials for example using `aws-switchrole` -- see below. In this case, don't forget to paste and eval.

```Bash
aws-switchrole --profile staging@cd --copy
```

