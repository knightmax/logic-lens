// Original Java file
import java.util.List;
import java.util.ArrayList;

public class OrderService {
    public double calculateTotal(List<OrderItem> items) {
        double total = 0;
        for (OrderItem item : items) {
            total += item.getPrice() * item.getQuantity();
        }
        return total;
    }

    public void processOrder(Order order) {
        double total = calculateTotal(order.getItems());
        if (total > 100) {
            order.setDiscount(total * 0.1);
        }
        order.setStatus("processed");
    }
}
