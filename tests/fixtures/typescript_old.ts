// Original TypeScript file for integration testing
export function processOrder(order: Order): Result {
    if (!order.items || order.items.length === 0) {
        throw new Error("Order must have items");
    }

    const total = order.items.reduce((sum, item) => sum + item.price * item.qty, 0);

    if (total > 1000) {
        applyDiscount(order, 0.1);
    }

    return {
        orderId: order.id,
        total: total,
        status: "processed",
    };
}

function applyDiscount(order: Order, rate: number): void {
    order.discount = order.items.reduce((sum, item) => sum + item.price * item.qty, 0) * rate;
}

function formatCurrency(amount: number): string {
    return `$${amount.toFixed(2)}`;
}
