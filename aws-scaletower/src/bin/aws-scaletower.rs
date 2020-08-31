use aws_scaletower::lambda_handler;
use lambda_runtime::lambda;

fn main() {
    lambda!(lambda_handler)
}
