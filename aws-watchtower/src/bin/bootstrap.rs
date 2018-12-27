use lambda_runtime::lambda;
use aws_watchtower::lambda_handler;

fn main() {
    lambda!(lambda_handler)
}
