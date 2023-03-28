import time


def sleep(duration, get_now=time.perf_counter):
    """
    Custom sleep function that works more accurate then time.sleep does.
    Taken from: https://stackoverflow.com/a/60185893/3684575
    :param duration: Duration to sleep (in seconds).
    :param get_now: Function to retrieve current time (time.perf_counter by default)
    :return:
    """
    now = get_now()
    end = now + duration
    while now < end:
        now = get_now()
