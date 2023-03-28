import time


class Benchmark:
    __marks = {}  # {0: [result, point1, point2, ...]}

    def start(self, point_name=None) -> float:
        if point_name is None:
            point_name = 0

        self.__marks[point_name] = [0, ((time.time_ns() / 1000000) / 1000)%60, 0]
        # print(f"start: {self.__marks}")

        return self.__marks[point_name]

    def end(self, point_name=None) -> tuple:
        if point_name is None:
            point_name = 0

        # print(self.__marks)
        self.__marks[point_name][2] = ((time.time_ns() / 1000000) / 1000)%60
        self.__marks[point_name][0] = self.__marks[point_name][2] - self.__marks[point_name][1]

        return self.__marks[point_name][0], f"{int(1E3 * self.__marks[point_name][0])}ms"

    def clear_points(self):
        self.__marks = {}
