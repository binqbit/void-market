use crate::{MathSession, Result, TfheValue, types::BinaryOp};

macro_rules! comparison {
    ($($name:ident: $op:ident),+ $(,)?) => {
        $(
            pub fn $name(&self, lhs: &TfheValue, rhs: &TfheValue) -> Result<TfheValue> {
                Ok(self.eval_binary(BinaryOp::$op, lhs, rhs, None)?.value)
            }
        )+
    };
}

impl MathSession {
    comparison!(
        eq: Eq,
        ne: Ne,
        lt: Lt,
        le: Le,
        gt: Gt,
        ge: Ge,
    );
}
