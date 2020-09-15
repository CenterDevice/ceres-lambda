use lambda_runtime::lambda;
use security_watchtower::lambda_handler;

fn main() {
    lambda!(lambda_handler)
}
