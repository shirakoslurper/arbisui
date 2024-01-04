// Our book manager manages state
// It applies state updates

// We get access when we request access....

// Update pool with sui event

pub async fn loop_blocks<'a>(
    run_data: &RunData, 
    exchanges: &Vec<Box<dyn Exchange>>, 
    market_graph: &mut MarketGraph<'a>,
    source_coin: &TypeTag
    // paths: Vec<Vec<&TypeTag>>
) -> Result<()> {

    let pool_state_changing_event_filters = exchanges
        .iter()
        .flat_map(|exchange| {
            exchange.event_filters()
        })
        .collect::<Vec<EventFilter>>();

    let mut subscribe_pool_state_changing_events = run_data
        .sui_client
        .event_api()
        .subscribe_event(
            EventFilter::Any(
                pool_state_changing_event_filters
            )
        )
        .await?;

    let event_struct_tag_to_pool_field = exchanges
        .iter()
        .flat_map(|exchange| {
            exchange.event_struct_tag_to_pool_field()
        })
        .collect::<HashMap<_, _>>();

    while Some(pool_event_result) = subscribe_pool_state_changing_events.next() {

    }

}