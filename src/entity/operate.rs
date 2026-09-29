use crate::utils::to_result_error;
pub struct Operator<T> {
    pub content: Option<T>,
    pub msg: String,
    pub is_success: bool,
}

impl<T> Operator<T> {
    pub fn ok(content: T) -> Operator<T> {
        Operator::<T> {
            content: Some(content),
            msg: String::from("success"),
            is_success: true,
        }
    }

    pub fn err(msg: &str) -> Operator<T> {
        Operator::<T> {
            content: None,
            msg: String::from(msg),
            is_success: false,
        }
    }

    pub fn to_result<'a>(self) -> Result<T, Box<dyn std::error::Error>> {
        if self.is_success {
            Ok(self.content.expect("content value is null"))
        } else {
            // 把字符串包装成Box<dyn Error>
            Err(Box::new(to_result_error::BizMsgError(self.msg)))
        }
    }
}
