use std::sync::{Arc, Mutex};

use chrono::{Local, Timelike};
use domain::{OrderIds, OrderType};
use engine::orderbook::OrderBook;

pub fn prune_good_for_day_orders(orderbook: Arc<Mutex<OrderBook>>) {
    loop {
        let _now = Local::now().hour(); // hrs on 24 hr clock format

        let mut order_ids: OrderIds = vec![];

        {
            let orderbook_gaurd = orderbook.lock().unwrap();

            for order_entry in orderbook_gaurd.orders.values() {
                let order_type = order_entry.order.borrow().get_order_type();
                if order_type != OrderType::GoodForDay {
                    continue;
                }
                let order_id = order_entry.order.borrow().get_order_id();
                order_ids.push(order_id);
            }
        }

        orderbook.lock().unwrap().cancel_orders(order_ids);
    }
}
