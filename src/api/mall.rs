//! 会员购 (Bilibili mall) 订单与物流接口的数据类型。

use serde::Deserialize;

/// 反序列化 `Vec<T>`，字段为 `null` 时当作空列表（B 站会员购接口常见行为）。
fn de_null_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    let opt = Option::<Vec<T>>::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

/// 订单列表中的单个订单。
#[derive(Debug, Clone, Deserialize)]
pub struct MallOrder {
    pub order_id: i64,
    #[serde(default)]
    pub shop_name: String,
    #[serde(default)]
    pub status_name: String,
    #[serde(default)]
    pub status_subname: String,
    #[serde(default)]
    pub total_desc: String,
    /// 支付金额（分）。
    #[serde(default)]
    pub pay_money: i64,
    #[serde(default)]
    pub money_label: String,
    /// 下单时间（Unix 秒）。
    #[serde(default)]
    pub order_ctime: i64,
    #[serde(default)]
    pub order_type: i64,
    #[serde(default)]
    pub express_fee: i64,
    /// 网页版订单详情/文件查看页 URL（用于浏览器兜底打开）。
    #[serde(default)]
    pub order_detail_url: String,
    /// 订单操作按钮（立即评价/查看文件等）。
    #[serde(default, deserialize_with = "de_null_vec")]
    pub op_json: Vec<MallOrderOp>,
    #[serde(default, deserialize_with = "de_null_vec")]
    pub rows: Vec<MallOrderRow>,
}

impl MallOrder {
    /// 优先返回「查看文件」操作按钮的 URL（工房数字商品订单专用），
    /// 找不到时回退到订单详情页 URL。
    pub fn file_view_url(&self) -> String {
        for op in &self.op_json {
            if op.name.contains("查看文件") && !op.url.is_empty() {
                return op.url.clone();
            }
        }
        self.order_detail_url.clone()
    }
}

/// 订单操作按钮（op_json 数组项）。
#[derive(Debug, Clone, Deserialize)]
pub struct MallOrderOp {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: String,
}

/// 订单中的商品行。
#[derive(Debug, Clone, Deserialize)]
pub struct MallOrderRow {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub logo: String,
    #[serde(default)]
    pub count: i64,
    #[serde(default)]
    pub money: i64,
    #[serde(default)]
    pub sku_id: i64,
    #[serde(default)]
    pub item_id: i64,
}

/// 订单列表接口响应（show.bilibili.com/api/ticket/ordercenter/list）。
#[derive(Debug, Deserialize)]
pub struct MallOrderListResp {
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub list: Option<Vec<MallOrder>>,
}

/// 物流概要（由 /mall-dayu/mall-trade/order/express/info 的 data[0] 映射而来）。
#[derive(Debug, Clone, Deserialize)]
pub struct MallExpressSummary {
    /// 快递公司显示名，如 "中通"。
    #[serde(default)]
    pub com_v: String,
    /// 快递单号。
    #[serde(default)]
    pub sno: String,
    /// 状态，如 "已签收"。
    #[serde(default)]
    pub state_v: String,
    /// 订单状态，如 "已完成"。
    #[serde(default)]
    pub status_v: String,
}

/// 物流轨迹中的一条记录。
#[derive(Debug, Clone, Deserialize)]
pub struct MallExpressTrace {
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub context: String,
}

/// 物流轨迹接口（POST /mall-dayu/mall-trade/order/express/info，JSON body）返回的单项。
#[derive(Debug, Deserialize)]
pub struct MallExpressTrackItem {
    #[serde(default)]
    pub com_v: String,
    #[serde(default)]
    pub sno: String,
    #[serde(default)]
    pub state_v: String,
    #[serde(default)]
    pub status_v: String,
    #[serde(default, deserialize_with = "de_null_vec")]
    pub detail: Vec<MallExpressTrace>,
}

/// 组合后的物流信息（概要 + 轨迹）。
#[derive(Debug, Clone)]
pub struct MallExpress {
    pub com_v: String,
    pub sno: String,
    pub state_v: String,
    pub status_v: String,
    pub traces: Vec<MallExpressTrace>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mall_order_accepts_null_vectors() {
        // B 站会员购接口对无操作按钮/无商品行的订单返回显式 null，
        // 反序列化必须把 null 当作空列表而不是报错。
        let json = r#"{
            "order_id": 8001350235561368,
            "shop_name": "圆桌补给站",
            "status_name": "已完成",
            "op_json": null,
            "rows": null
        }"#;
        let order: MallOrder = serde_json::from_str(json).expect("null vectors must parse");
        assert!(order.op_json.is_empty());
        assert!(order.rows.is_empty());
    }

    #[test]
    fn mall_track_accepts_null_detail() {
        let json = r#"{
            "com_v": "圆通",
            "sno": "YT2591002491909",
            "state_v": "已签收",
            "status_v": "已完成",
            "detail": null
        }"#;
        let item: MallExpressTrackItem = serde_json::from_str(json).expect("null detail must parse");
        assert!(item.detail.is_empty());
    }
}

