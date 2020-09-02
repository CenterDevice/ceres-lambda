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

// Filters for specific instances, not all
instance_name_filter = "<Tags:Name filter>"
// Looks back <min> minutes to compute linear regression
look_back_min = <min>
// Enable linear regression to compute ETA for when instance runs out of burts
use_linear_regression = <true|false>
// Burst balance limit after which the instance will be termianted
burst_balance_limit = <number between 100 and 0>
// Limit in min after which the instance will be termianted
eta_limit_min = <min>
// Enabled instance termination
terminate = <true|false>
```

### Validate Configuration

This crate contains a executable that validates an encrypted configuration file called `validate-config-scaletower`. Please check the help information for details. For decryption valid AWS credentials in environment variables are required. 

In this example, the encrypted as well as the decrypted configurations are printed and checked:

```Bash
cargo run --bin validate-config-scaletower ~INFRA/AWS/staging/logimon/terraform/resources/lambda/packages/config_enc_aws-scaletower.conf -vv
```

You can set the AWS credentials for example using `aws-switchrole` -- see below. In this case, don't forget to paste and eval.

```Bash
aws-switchrole --profile staging@cd --copy
```

