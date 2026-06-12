use crate::recorder::Action;
use std::time::Duration;
use chromiumoxide::browser::Browser;
use chromiumoxide::layout::Point;

/// Replays recorded actions in a browser with precise timing control.
///
/// The `ActionReplayer` can execute a sequence of `Action` enums in a
/// live browser instance, simulating user interaction for testing or cloning.
pub struct ActionReplayer {
    /// Speed multiplier for playback (1.0 is real-time).
    pub speed: f32
}

impl ActionReplayer {
    /// Create a new action replayer with the given speed multiplier.
    pub fn new(speed: f32) -> Self { Self { speed } }

    /// Replay a single action in the browser.
    ///
    /// This method uses the `chromiumoxide` library to send low-level
    /// CDP commands to the browser. It handles mouse movements, clicks,
    /// scrolling, and keyboard input.
    ///
    /// # Errors
    /// Returns an error if the browser session is closed or if the
    /// action cannot be simulated.
    pub async fn replay_action(&self, action: &Action, browser: &mut Browser) -> Result<(), anyhow::Error> {
        let wait_time = Duration::from_secs_f32(1.0 / self.speed);
        tokio::time::sleep(wait_time).await;

        let pages = browser.pages().await?;
        let page = pages.last().ok_or_else(|| anyhow::anyhow!("No active page for replay"))?;

        match action {
            Action::MouseMove { x, y } => {
                page.move_mouse(Point::new(*x, *y)).await?;
            }
            Action::Click { x, y, selector, .. } => {
                // Robust element re-finding: if a selector is provided, try to find the element first
                if let Some(sel) = selector {
                    if let Ok(el) = page.find_element(sel).await {
                        el.click().await?;
                    } else {
                        // Fallback to coordinate click if element not found by selector
                        page.click(Point::new(*x, *y)).await?;
                    }
                } else {
                    page.click(Point::new(*x, *y)).await?;
                }
            }
            Action::Scroll { delta_x, delta_y } => {
                page.evaluate(format!("window.scrollBy({}, {})", delta_x, delta_y)).await?;
            }
            Action::KeyPress { key } => {
                // Simulate real keyboard events in the browser context
                page.evaluate(format!("
                    const event = new KeyboardEvent('keydown', {{
                        key: '{}',
                        bubbles: true,
                        cancelable: true
                    }});
                    document.activeElement.dispatchEvent(event);
                ", key)).await?;
            }
        }
        Ok(())
    }

    /// Replay a sequence of actions.
    pub async fn replay_sequence(&self, actions: &[Action], browser: &mut Browser) -> Result<(), anyhow::Error> {
        for action in actions {
            self.replay_action(action, browser).await?;
        }
        Ok(())
    }
}
