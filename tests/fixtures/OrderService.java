// AI-modified Java file
import java.util.List;
import java.util.ArrayList;
import java.util.stream.Collectors;

public class OrderService {
    public double calculateTotal(List<OrderItem> items) {
        return items.stream()
            .mapToDouble(item -> item.getPrice() * item.getQuantity())
            .sum();
    }

    public void processOrder(Order order) {
        double total = calculateTotal(order.getItems());
        if (total > 50) {
            order.setDiscount(total * 0.15);
        }
        order.setStatus("processed");
    }

    public void cancelOrder(Order order) {
        // TODO: implement
    }
}
