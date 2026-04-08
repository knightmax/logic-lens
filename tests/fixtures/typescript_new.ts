// AI-modified TypeScript file for integration testing
import { Logger } from 'custom-logger';

export async function processOrder(order: Order): Promise<Result> {
    // TODO: implement validation
    const total = order.items.reduce((sum, item) => sum + item.price * item.qty, 0);

    if (total > 500) {
        applyDiscount(order, 0.15);
    }

    const data = await fetch('/api/orders', {
        method: 'POST',
        body: JSON.stringify(order),
    });

    return {
        orderId: order.id,
        total: total,
        status: "processed",
    };
}

function applyDiscount(order: Order, rate: number): void {
    order.discount = order.items.reduce((sum, item) => sum + item.price * item.qty, 0) * rate;
}

function formatAmount(amount: number): string {
    return `$${amount.toFixed(2)}`;
}

function validateOrder(order: Order): boolean {
    // ...
}
