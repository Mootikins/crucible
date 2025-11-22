# E-Commerce Order Processing

This workflow handles customer orders from validation through fulfillment, with parallel processing for inventory and payment.

## Receive Order @order-api #orders
API endpoint receives customer order.
order_request::JSON → validated_order::Order

```session-toon
execution[5]{phase,agent,channel,input,output,duration_ms,status,tokens_used,customer_id}:
 ReceiveOrder,order-api,orders,req_001.json,order_001,120,success,340,cust_789
 ReceiveOrder,order-api,orders,req_002.json,order_002,95,success,320,cust_456
 ReceiveOrder,order-api,orders,req_003.json,order_003,110,success,335,cust_123
 ReceiveOrder,order-api,orders,req_004.json,error,80,failed,280,cust_999
 ReceiveOrder,order-api,orders,req_005.json,order_005,125,success,350,cust_234
```

---

## Check Inventory @inventory-service #warehouse
Verifies all items are in stock.
validated_order → inventory_check::InventoryResult

```session-toon
execution[4]{phase,agent,channel,input,output,duration_ms,status,tokens_used,items_count}:
 CheckInventory,inventory-service,warehouse,order_001,inventory_ok_001,340,success,420,3
 CheckInventory,inventory-service,warehouse,order_002,inventory_ok_002,380,success,440,5
 CheckInventory,inventory-service,warehouse,order_003,inventory_fail_003,290,failed,380,2
 CheckInventory,inventory-service,warehouse,order_005,inventory_ok_005,350,success,430,4
```

---

## Process Payment @payment-service #payments !
Critical: Charges customer's payment method.
validated_order → payment_result::PaymentReceipt

This step runs in parallel with inventory check for faster processing.

```session-toon
execution[3]{phase,agent,channel,input,output,duration_ms,status,tokens_used,amount_usd}:
 ProcessPayment,payment-service,payments,order_001,receipt_001,1240,success,520,89.99
 ProcessPayment,payment-service,payments,order_002,receipt_002,1580,success,580,145.50
 ProcessPayment,payment-service,payments,order_005,receipt_005,1320,success,540,67.25
```

---

## Reserve Inventory @inventory-service #warehouse !
Critical: Reserves items for order.
inventory_check, payment_result → reservation::Reservation

Only proceeds if both inventory check and payment succeeded.

```session-toon
execution[3]{phase,agent,channel,input,output,duration_ms,status,tokens_used}:
 ReserveInventory,inventory-service,warehouse,order_001,reservation_001,680,success,390
 ReserveInventory,inventory-service,warehouse,order_002,reservation_002,720,success,410
 ReserveInventory,inventory-service,warehouse,order_005,reservation_005,690,success,395
```

---

## Create Shipment @fulfillment-service #warehouse
Generates shipping label and notifies carrier.
reservation → tracking_number::String

```session-toon
execution[3]{phase,agent,channel,input,output,duration_ms,status,tokens_used,carrier}:
 CreateShipment,fulfillment-service,warehouse,reservation_001,track_1Z999,2100,success,680,UPS
 CreateShipment,fulfillment-service,warehouse,reservation_002,track_1Z888,2250,success,720,FedEx
 CreateShipment,fulfillment-service,warehouse,reservation_005,track_1Z777,2050,success,670,USPS
```

---

## Notify Customer @notification-service #communications
Sends order confirmation and tracking info to customer.
tracking_number, payment_result → notification_sent::Boolean

```session-toon
execution[3]{phase,agent,channel,input,output,duration_ms,status,tokens_used,email_sent}:
 NotifyCustomer,notification-service,communications,order_001,sent,450,success,320,true
 NotifyCustomer,notification-service,communications,order_002,sent,480,success,335,true
 NotifyCustomer,notification-service,communications,order_005,sent,460,success,325,true
```

---

## Handle Failed Orders ?
Optional: Processes orders that failed inventory or payment.
failed_orders → retry_scheduled::Boolean

This step only runs if there were failures in previous phases.

```session-toon
execution[2]{phase,agent,channel,input,output,duration_ms,status,tokens_used,retry_count}:
 HandleFailedOrders,order-api,orders,order_003,retry_1,850,success,420,1
 HandleFailedOrders,order-api,orders,order_004,retry_1,790,success,390,1
```

## Workflow Summary

**Execution Statistics**:
- Total orders received: 5
- Successful completions: 3 (60%)
- Failed inventory check: 1 (order_003)
- Failed initial validation: 1 (order_004)
- Average processing time: ~8.5 seconds per successful order

**Parallel Processing Benefits**:
- Inventory check and payment processing ran concurrently
- Saved ~1.2 seconds per order vs. sequential processing
- Critical steps (payment, inventory) marked with `!` for failure handling

**Next Steps**:
- Retry failed orders after inventory replenishment
- Monitor payment gateway response times (currently averaging 1.38s)
- Optimize shipping label generation (slowest step at ~2.1s)
